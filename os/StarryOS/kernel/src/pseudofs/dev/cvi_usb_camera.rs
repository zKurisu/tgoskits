use core::{any::Any, time::Duration};

use ax_errno::{AxError, LinuxError};
use ax_memory_addr::{PhysAddr, VirtAddr};
use ax_runtime::hal::{
    mem::virt_to_phys,
    time::{busy_wait, monotonic_time_nanos},
};
use ax_sync::Mutex;
use axfs_ng_vfs::{NodeFlags, VfsResult};
use sg200x_bsp::{
    gpio::{Direction, GPIO, GPIO1_BASE},
    jpu::{
        JpuDecoder,
        regs::{JPU_REG_BASE, VC_REG_BASE},
    },
    pinmux::{FMUX_USB_VBUS_DET, Pinmux},
    soc::{
        CLKGEN_BASE, CV182X_USB2_PHY_BASE, DWC2_BASE, FMUX_BASE, IOBLK_BASE, IOBLK_GRTC_BASE,
        TOP_BASE,
    },
    usb::{
        self,
        class::uvc,
        error::UsbError,
        host::{self, UvcEnumerated, dwc2, dwc2::ep0 as dwc2_ep0},
    },
};
use starry_vm::{VmMutPtr, vm_write_slice};
use tock_registers::interfaces::Writeable;

use crate::pseudofs::DeviceOps;

const IOBLK_G1_USB_VBUS_DET_OFF: usize = 0x020;

const VBUS_GPIO_PIN: u8 = 6;
const VBUS_GPIO_ACTIVE_HIGH: bool = true;

/// MMIO span of the TOP control block. The PHY ID-pad reset register lives at
/// `TOP_BASE + 0x3000`, so a single 4K page is not enough — map four pages.
const TOP_MMIO_SIZE: usize = 0x4000;
/// MMIO span for the single-page register blocks (CLKGEN, FMUX, IOBLK, GRTC,
/// GPIO, DWC2 controller, USB2 PHY). Each block's registers fit within one 4K
/// page; FMUX/IOBLK share a page so their mappings coincide (idempotent).
const REG_MMIO_SIZE: usize = 0x1000;

/// Map a physical MMIO region into the kernel address space and return its
/// virtual base. Unlike `phys_to_virt`, this works on dynamic platforms where
/// `PHYS_VIRT_OFFSET == 0` and there is no static linear MMIO window — `iomap`
/// installs a real device mapping and is idempotent for already-mapped pages.
fn iomap_usize(paddr: usize, size: usize) -> usize {
    ax_mm::iomap(PhysAddr::from_usize(paddr), size)
        .unwrap_or_else(|err| panic!("failed to iomap MMIO at {paddr:#x}+{size:#x}: {err:?}"))
        .as_usize()
}

const CAMERA_FORMAT_MJPEG: u8 = 1;
const MIN_VALID_JPEG_BYTES: usize = 4096;
const MAX_CAPTURE_TRIES: u32 = 8;
/// Default resolution cap (640×480 = 307200 pixels) guiding UVC frame selection.
const DEFAULT_RESOLUTION: u32 = 640 * 480;

pub const CVI_CAMERA_IOCTL_INIT: u32 = 1;
pub const CVI_CAMERA_IOCTL_GET_INFO: u32 = 2;
pub const CVI_CAMERA_IOCTL_GET_FRAME: u32 = 3;
pub const CVI_CAMERA_IOCTL_GET_YUV_FRAME: u32 = 4;
/// Power-cycle the camera VBUS and perform a full hardware re-initialization.
/// This is the strongest recovery mechanism — use when persistent EIO cannot
/// be fixed by INIT alone.
pub const CVI_CAMERA_IOCTL_HARD_RESET: u32 = 5;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraInfo {
    pub width: u16,
    pub height: u16,
    /// 1 means MJPEG.
    pub format: u8,
    pub connected: u8,
}

struct UsbCameraSession {
    cam: UvcEnumerated,
    sel: uvc::UvcStreamSelection,
}

/// After this many successful decodes the JPU is transparently recycled
/// (Drop + fresh construction) so that its internal state (BBC/GBU
/// pointers, FIFOs, error counters) is reset via `hardware_init_at()`.
/// The C reference driver calls `JPU_SWReset()` between *every* frame;
/// we amortise a single full reset over this many frames to keep the
/// overhead negligible (< 0.1 % at 2.5 fps).
const JPU_RECYCLE_INTERVAL: u32 = 100;

#[derive(Default)]
struct UsbCameraState {
    session: Option<UsbCameraSession>,
    jpu: Option<JpuDecoder>,
    jpu_decode_count: u32,
}

fn jpu_dma_to_phys(v: usize) -> usize {
    virt_to_phys(VirtAddr::from(v)).as_usize()
}

pub struct CviCamera {
    state: Mutex<UsbCameraState>,
}

fn ep0_dma_virt_to_phys(p: *const u8) -> u32 {
    virt_to_phys(VirtAddr::from(p as usize)).as_usize() as u32
}

unsafe fn enable_usb_clocks_cv181x() {
    let b = iomap_usize(CLKGEN_BASE, REG_MMIO_SIZE);
    let en1 = (b + 0x004) as *mut u32;
    let en2 = (b + 0x008) as *mut u32;
    let byp0 = (b + 0x030) as *mut u32;
    unsafe {
        let v1_pre = core::ptr::read_volatile(en1);
        let v2_pre = core::ptr::read_volatile(en2);
        let byp_pre = core::ptr::read_volatile(byp0);
        core::ptr::write_volatile(en1, v1_pre | (0xFu32 << 28));
        core::ptr::write_volatile(en2, v2_pre | 1u32);
        core::ptr::write_volatile(byp0, byp_pre & !((1u32 << 17) | (1u32 << 18)));
    }
}

/// PHY ID pad toggle workaround: switch to device mode first, then host mode.
unsafe fn cvitek_usb_top_host_bringup() {
    let top = iomap_usize(TOP_BASE, TOP_MMIO_SIZE);
    let rst = (top + 0x3000) as *mut u32;
    unsafe {
        let v = core::ptr::read_volatile(rst);
        core::ptr::write_volatile(rst, v & !(1 << 11));
        busy_wait(Duration::from_micros(50));
        core::ptr::write_volatile(rst, v | (1 << 11));
        busy_wait(Duration::from_micros(50));

        let usb_pin = (top + 0x48) as *mut u32;
        let x = core::ptr::read_volatile(usb_pin);
        let dev_mode = (x & !0xC0u32) | 0xC0u32 | 0x01u32;
        core::ptr::write_volatile(usb_pin, dev_mode);
        busy_wait(Duration::from_micros(1000));
        let host_mode = (x & !0xC0u32) | 0x40u32 | 0x01u32;
        core::ptr::write_volatile(usb_pin, host_mode);
        busy_wait(Duration::from_micros(1000));

        let eco = (top + 0xB4) as *mut u32;
        core::ptr::write_volatile(eco, core::ptr::read_volatile(eco) | 0x80);
    }
}

fn pinmux_usb_vbus_det_gpio_output_prep() {
    let fmux_vaddr = iomap_usize(FMUX_BASE, REG_MMIO_SIZE);
    let ioblk_vaddr = iomap_usize(IOBLK_BASE, REG_MMIO_SIZE);
    let ioblk_grtc_vaddr = iomap_usize(IOBLK_GRTC_BASE, REG_MMIO_SIZE);
    let pinmux = unsafe { Pinmux::new(fmux_vaddr, ioblk_vaddr, ioblk_grtc_vaddr) };
    pinmux
        .fmux()
        .usb_vbus_det
        .write(FMUX_USB_VBUS_DET::FSEL::XGPIOB_6);
    let r = (ioblk_vaddr + IOBLK_G1_USB_VBUS_DET_OFF) as *mut u32;
    unsafe {
        let v = core::ptr::read_volatile(r);
        core::ptr::write_volatile(r, v | (7 << 5));
    }
}

fn enable_usb_vbus_gpio() {
    set_vbus(true);
}

fn disable_usb_vbus_gpio() {
    set_vbus(false);
}

fn set_vbus(enable: bool) {
    let gpio = unsafe { GPIO::new(iomap_usize(GPIO1_BASE, REG_MMIO_SIZE)) };
    gpio.pin(VBUS_GPIO_PIN).set_direction(Direction::Output);
    gpio.pin(VBUS_GPIO_PIN).set(if VBUS_GPIO_ACTIVE_HIGH {
        enable
    } else {
        !enable
    });
}

/// Power-cycle the camera over VBUS: off → wait → on → wait for
/// device to settle.  Caller should re-init afterwards.
fn power_cycle_camera_vbus() {
    info!("cvi-camera: power-cycling VBUS ...");
    disable_usb_vbus_gpio();
    ax_task::sleep(Duration::from_micros(500_000)); // 500 ms off
    enable_usb_vbus_gpio();
    ax_task::sleep(Duration::from_micros(2_000_000)); // 2 s on for device init
    info!("cvi-camera: VBUS power-cycle complete");
}

fn map_usb_init_error(e: UsbError) -> &'static str {
    match e {
        UsbError::NotImplemented => "no VS bulk/isoch video endpoint found",
        _ => "failed to parse UVC stream parameters",
    }
}

fn init_usb_camera() -> Result<UsbCameraSession, &'static str> {
    unsafe {
        enable_usb_clocks_cv181x();
        cvitek_usb_top_host_bringup();
    }
    pinmux_usb_vbus_det_gpio_output_prep();
    enable_usb_vbus_gpio();
    ax_task::sleep(Duration::from_micros(2_000_000));

    usb::set_dwc2_base_virt(iomap_usize(DWC2_BASE, REG_MMIO_SIZE));
    usb::set_cv182x_phy_base_virt(iomap_usize(CV182X_USB2_PHY_BASE, REG_MMIO_SIZE));
    usb::set_usb_dma_to_phys_fn(Some(ep0_dma_virt_to_phys));

    unsafe {
        dwc2::dwc2_probe().map_err(|e| {
            warn!("cvi-camera: DWC2 probe failed: {e:?}");
            "DWC2 probe failed"
        })?;
    }

    let mut last_err = None;
    let extras = (0..4)
        .find_map(|attempt| {
            if attempt > 0 {
                busy_wait(Duration::from_micros(1_500_000 * attempt as u64));
            }
            match host::enumerate_topology_only() {
                Ok(extras) => Some(extras),
                Err(e) => {
                    warn!("cvi-camera: USB enumerate failed #{}: {:?}", attempt + 1, e);
                    last_err = Some(e);
                    None
                }
            }
        })
        .ok_or_else(|| {
            warn!(
                "cvi-camera: USB enumerate retries exhausted: {:?}",
                last_err
            );
            "USB topology enumeration failed"
        })?;

    let cam = extras.uvc.ok_or("no UVC camera detected")?;
    info!(
        "cvi-camera: UVC addr={} VID={:04x} PID={:04x} ep0_mps={}",
        cam.addr, cam.vid, cam.pid, cam.ep0_mps
    );

    let dev = u32::from(cam.addr);
    let ep0 = cam.ep0_mps;
    let cfg_buf = uvc::read_configuration_descriptor(dev, ep0, 1).map_err(|e| {
        warn!("cvi-camera: read configuration descriptor failed: {e:?}");
        "failed to read configuration descriptor"
    })?;
    let cfg_total = u16::from_le_bytes([cfg_buf[2], cfg_buf[3]]) as usize;
    let cfg = &cfg_buf[..cfg_total.min(cfg_buf.len())];
    uvc::set_preferred_max_pixels(DEFAULT_RESOLUTION);
    let mut sel = uvc::parse_uvc_video_stream(cfg, cfg_total).map_err(|e| {
        warn!("cvi-camera: parse UVC video stream failed: {e:?}");
        map_usb_init_error(e)
    })?;

    if let Some(entities) = uvc::parse_uvc_control_entities(cfg, cfg_total) {
        let tune = uvc::UvcImageTuning {
            brightness: Some(96),
            ..uvc::UvcImageTuning::default()
        };
        let _ = uvc::uvc_init_camera_controls(dev, ep0, &entities, &tune);
    }

    uvc::uvc_start_video_stream(dev, ep0, &mut sel).map_err(|e| {
        warn!("cvi-camera: start UVC stream failed: {e:?}");
        "UVC PROBE/COMMIT or SET_INTERFACE failed"
    })?;
    info!(
        "cvi-camera: stream ready {}x{} payload={} frame_size={}",
        sel.frame_w, sel.frame_h, sel.negotiated_payload_size, sel.negotiated_frame_size
    );

    // Warm-up frame: discard the first capture after stream start so the
    // isochronous pipeline and DMA buffer are ready for real reads.
    let _ = uvc::uvc_capture_one_frame(dev, ep0, &sel);
    Ok(UsbCameraSession { cam, sel })
}

fn capture_frame(session: &UsbCameraSession) -> Result<&'static [u8], &'static str> {
    let dev = u32::from(session.cam.addr);
    let ep0 = session.cam.ep0_mps;
    let mut last_n = 0;
    let mut last_msg = None;
    for attempt in 0..MAX_CAPTURE_TRIES {
        let n = uvc::uvc_capture_one_frame(dev, ep0, &session.sel).map_err(|e| {
            warn!("cvi-camera: capture failed: {e:?}");
            trace_camera_capture_frame(false, attempt, 0);
            "frame capture failed"
        })?;
        last_n = n;
        let frame =
            dwc2_ep0::dma_rx_slice(uvc::UVC_ASSEMBLED_JPEG_DMA_OFF, n).ok_or_else(|| {
                trace_camera_capture_frame(false, attempt, n as u32);
                "DMA slice out of bounds"
            })?;
        let starts_jpeg = n >= 2 && frame[0] == 0xff && frame[1] == 0xd8;
        let ends_jpeg = n >= 2 && frame[n - 2] == 0xff && frame[n - 1] == 0xd9;
        if starts_jpeg && ends_jpeg && n >= MIN_VALID_JPEG_BYTES {
            trace_camera_capture_frame(true, attempt + 1, n as u32);
            return Ok(frame);
        }
        last_msg = Some(if !starts_jpeg {
            "first bytes are not ff d8"
        } else if !ends_jpeg {
            "last bytes are not ff d9 (truncated)"
        } else {
            "frame too small"
        });
        warn!(
            "cvi-camera: invalid frame (try #{}/{}, size={}, {}), reset FID",
            attempt + 1,
            MAX_CAPTURE_TRIES,
            n,
            last_msg.unwrap_or("?")
        );
        uvc::reset_frame_continuity();
    }
    warn!(
        "cvi-camera: no complete JPEG after {} retries, size={} {}",
        MAX_CAPTURE_TRIES,
        last_n,
        last_msg.unwrap_or("?")
    );
    trace_camera_capture_frame(false, MAX_CAPTURE_TRIES, last_n as u32);
    dwc2_ep0::dma_rx_slice(uvc::UVC_ASSEMBLED_JPEG_DMA_OFF, last_n).ok_or("DMA slice out of bounds")
}

impl UsbCameraState {
    /// Clear the USB camera session and JPU state so the next
    /// `ensure_initialized` call performs a full hardware re-init.
    fn reset(&mut self) {
        self.session = None;
        self.jpu = None;
        self.jpu_decode_count = 0;
    }

    fn ensure_initialized(&mut self) -> VfsResult<()> {
        if self.session.is_none() {
            match init_usb_camera() {
                Ok(session) => {
                    self.session = Some(session);
                    trace_camera_ensure_init(true);
                }
                Err(msg) => {
                    warn!("cvi-camera: init failed: {msg}");
                    trace_camera_ensure_init(false);
                    return Err(AxError::Io);
                }
            }
        }
        Ok(())
    }

    fn info(&mut self) -> VfsResult<CameraInfo> {
        self.ensure_initialized()?;
        let session = self.session.as_ref().ok_or(AxError::BadState)?;
        Ok(CameraInfo {
            width: session.sel.frame_w,
            height: session.sel.frame_h,
            format: CAMERA_FORMAT_MJPEG,
            connected: 1,
        })
    }

    fn frame(&mut self) -> VfsResult<&'static [u8]> {
        self.ensure_initialized()?;
        capture_frame(self.session.as_ref().ok_or(AxError::BadState)?).map_err(|msg| {
            warn!("cvi-camera: capture failed: {msg}");
            AxError::Io
        })
    }

    fn ensure_jpu(&mut self) -> VfsResult<&mut JpuDecoder> {
        // Proactively recycle the JPU every N successful decodes so its
        // internal hardware state (BBC/GBU pointers, FIFOs, error counters)
        // does not accumulate drift over hundreds of frames.  The C
        // reference driver does this between *every* frame via JPU_SWReset;
        // amortising to one Reset+Init every 100 frames keeps the overhead
        // negligible (< 0.1 % at 2.5 fps).
        if self.jpu.is_some() && self.jpu_decode_count >= JPU_RECYCLE_INTERVAL {
            debug!(
                "cvi-camera: recycling JPU after {} decodes",
                self.jpu_decode_count
            );
            self.jpu = None;
            self.jpu_decode_count = 0;
        }
        if self.jpu.is_none() {
            let jpu_v = iomap_usize(JPU_REG_BASE, REG_MMIO_SIZE);
            let top_v = iomap_usize(TOP_BASE, TOP_MMIO_SIZE);
            let vc_v = iomap_usize(VC_REG_BASE, REG_MMIO_SIZE);
            let decoder = unsafe {
                JpuDecoder::new_at(jpu_v, top_v, vc_v, jpu_dma_to_phys).map_err(|e| {
                    warn!("cvi-camera: JPU init failed: {e}");
                    AxError::Io
                })?
            };
            self.jpu = Some(decoder);
        }
        Ok(self.jpu.as_mut().unwrap())
    }

    fn yuv_frame(&mut self) -> VfsResult<&'static [u8]> {
        let jpeg = self.frame()?;
        let result = self.ensure_jpu()?.decode(jpeg).map_err(|e| {
            warn!("cvi-camera: JPU decode failed ({e}), resetting JPU (next frame will re-init)");
            self.jpu = None;
            self.jpu_decode_count = 0;
            AxError::Io
        })?;
        self.jpu_decode_count = self.jpu_decode_count.saturating_add(1);
        info!(
            "cvi-camera: JPU decode OK {}x{} yuv={} bytes",
            result.width,
            result.height,
            result.yuv_data.len()
        );
        Ok(result.yuv_data)
    }
}

impl CviCamera {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(UsbCameraState::default()),
        }
    }
}

ktracepoint::define_event_trace!(
    cvi_camera_ioctl,
    TP_kops(crate::tracepoint::KernelTraceAux),
    TP_system(camera),
    TP_PROTO(cmd: u32, elapsed_us: u64, ok: bool, ret: i32),
    TP_STRUCT__entry{
        cmd: u32,
        elapsed_us: u64,
        ok: u8,
        ret: i32,
    },
    TP_fast_assign{
        cmd: cmd,
        elapsed_us: elapsed_us,
        ok: ok as u8,
        ret: ret,
    },
    TP_ident(__entry),
    TP_printk({
        let name = match __entry.cmd {
            CVI_CAMERA_IOCTL_INIT => "INIT",
            CVI_CAMERA_IOCTL_GET_INFO => "GET_INFO",
            CVI_CAMERA_IOCTL_GET_FRAME => "GET_FRAME",
            CVI_CAMERA_IOCTL_GET_YUV_FRAME => "GET_YUV_FRAME",
            CVI_CAMERA_IOCTL_HARD_RESET => "HARD_RESET",
            _ => "?",
        };
        alloc::format!(
            "{} (cmd={}) elapsed={}us ok={} ret={}",
            name, __entry.cmd, __entry.elapsed_us, __entry.ok != 0, __entry.ret
        )
    })
);

ktracepoint::define_event_trace!(
    camera_ensure_init,
    TP_kops(crate::tracepoint::KernelTraceAux),
    TP_system(camera),
    TP_PROTO(ok: bool),
    TP_STRUCT__entry{
        ok: u8,
    },
    TP_fast_assign{
        ok: ok as u8,
    },
    TP_ident(__entry),
    TP_printk({
        alloc::format!("initialized={}", __entry.ok != 0)
    })
);

ktracepoint::define_event_trace!(
    camera_capture_frame,
    TP_kops(crate::tracepoint::KernelTraceAux),
    TP_system(camera),
    TP_PROTO(ok: bool, attempts: u32, bytes: u32),
    TP_STRUCT__entry{
        ok: u8,
        attempts: u32,
        bytes: u32,
    },
    TP_fast_assign{
        ok: ok as u8,
        attempts: attempts,
        bytes: bytes,
    },
    TP_ident(__entry),
    TP_printk({
        alloc::format!("ok={} attempts={} bytes={}", __entry.ok != 0, __entry.attempts, __entry.bytes)
    })
);

impl DeviceOps for CviCamera {
    fn read_at(&self, _buf: &mut [u8], _offset: u64) -> VfsResult<usize> {
        Ok(0)
    }

    fn write_at(&self, buf: &[u8], _offset: u64) -> VfsResult<usize> {
        Ok(buf.len())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn flags(&self) -> NodeFlags {
        NodeFlags::NON_CACHEABLE | NodeFlags::STREAM
    }

    fn close(&self, _exclusive: bool) {
        info!("cvi-camera: close — clearing session and JPU state");
        self.state.lock().reset();
    }

    fn ioctl(&self, cmd: u32, arg: usize) -> VfsResult<usize> {
        let start = monotonic_time_nanos();
        let result = match cmd {
            CVI_CAMERA_IOCTL_INIT => {
                // Clear any stale session so full hardware init runs again.
                self.state.lock().reset();
                self.state.lock().ensure_initialized().map(|_| 0)
            }
            CVI_CAMERA_IOCTL_HARD_RESET => {
                // VBUS power-cycle: physically power-cycle the camera
                // module, then do a full hardware init from scratch.
                info!("cvi-camera: HARD_RESET ioctl — power-cycling VBUS ...");
                self.state.lock().reset();
                // Drop the lock before the long sleep so other ops can fail
                // gracefully rather than blocking.
                drop(self.state.lock());
                power_cycle_camera_vbus();
                self.state.lock().ensure_initialized().map(|_| 0)
            }
            CVI_CAMERA_IOCTL_GET_INFO => {
                let info = self.state.lock().info()?;
                (arg as *mut CameraInfo).vm_write(info)?;
                Ok(0)
            }
            CVI_CAMERA_IOCTL_GET_FRAME => {
                let frame = self.state.lock().frame()?;
                vm_write_slice(arg as *mut u8, frame)?;
                Ok(frame.len())
            }
            CVI_CAMERA_IOCTL_GET_YUV_FRAME => {
                let yuv = self.state.lock().yuv_frame()?;
                vm_write_slice(arg as *mut u8, yuv)?;
                Ok(yuv.len())
            }
            _ => Err(AxError::InvalidInput),
        };
        let elapsed_us = (monotonic_time_nanos() - start) / 1_000;
        let ret = match &result {
            Ok(_) => 0,
            Err(e) => -(LinuxError::from(*e).code()),
        };
        trace_cvi_camera_ioctl(cmd, elapsed_us, result.is_ok(), ret);
        result
    }
}
