use alloc::{string::String, sync::Arc, vec::Vec};
use core::{
    any::Any,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

use ax_errno::{AxError, LinuxError};
use ax_memory_addr::{PhysAddr, VirtAddr};
use ax_runtime::hal::{
    mem::virt_to_phys,
    time::{busy_wait, monotonic_time_nanos},
};
use ax_sync::Mutex;
use ax_task::WaitQueue;
use axfs_ng_vfs::{NodeFlags, VfsResult};
use bytemuck::AnyBitPattern;
use sg200x_bsp::{
    gpio::{Direction, GPIO, GPIO1_BASE},
    jpu::{
        DecodeOutput, JpuDecoder,
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
use sg2002_tpu::ion::IonBuffer;
use starry_vm::{VmMutPtr, VmPtr, vm_write_slice};
use tock_registers::interfaces::Writeable;

use crate::{
    file::{get_file_like, ion::IonBufferFile},
    pseudofs::DeviceOps,
};

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
const CAMERA_FORMAT_YUV420_PLANAR: u8 = 2;
const CAMERA_FORMAT_YUV422_PLANAR: u8 = 3;
const CAMERA_FORMAT_YUV440_PLANAR: u8 = 4;
const CAMERA_FORMAT_YUV444_PLANAR: u8 = 5;
const CAMERA_FORMAT_YUV400: u8 = 6;
const CAMERA_FORMAT_NV12: u8 = 7;
const MIN_VALID_JPEG_BYTES: usize = 4096;
const MAX_CAPTURE_TRIES: u32 = 8;
const ASYNC_STOP_WAIT_TRIES: usize = 200;
const ASYNC_STOP_WAIT_INTERVAL: Duration = Duration::from_millis(10);
const ASYNC_ERROR_BACKOFF: Duration = Duration::from_millis(20);
/// A syntactically complete UVC JPEG can still be rejected by the hardware
/// decoder. Drop that frame, reset the JPU and retry newer frames without
/// terminating the real-time stream on one transient camera/JPU mismatch.
const MAX_JPU_DECODE_TRIES: u32 = 3;
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
/// Start the background latest-frame producer. The operation is idempotent.
pub const CVI_CAMERA_IOCTL_START_ASYNC: u32 = 6;
/// Stop the background producer and wait for an in-flight capture to finish.
pub const CVI_CAMERA_IOCTL_STOP_ASYNC: u32 = 7;
/// Read a newer MJPEG frame through [`CameraFrameRequest`].
pub const CVI_CAMERA_IOCTL_GET_LATEST_FRAME: u32 = 8;
/// Read capture counters and the most recent timing sample.
pub const CVI_CAMERA_IOCTL_GET_CAPTURE_STATS: u32 = 9;
/// Reset capture counters without stopping the stream.
pub const CVI_CAMERA_IOCTL_RESET_CAPTURE_STATS: u32 = 10;
/// Decode the latest asynchronously captured JPEG with the JPU and return
/// planar YUV data through [`CameraFrameRequest`]. The output format follows
/// the JPEG SOF sampling factors and is reported in `CameraFrameRequest.format`.
pub const CVI_CAMERA_IOCTL_GET_LATEST_YUV_FRAME: u32 = 11;
/// Decode the latest asynchronously captured 4:2:0 JPEG directly to NV12.
/// A non-4:2:0 JPEG is rejected because JPU interleave preserves sampling
/// (4:2:2 plus interleave is NV16, not NV12).
pub const CVI_CAMERA_IOCTL_GET_LATEST_NV12_FRAME: u32 = 12;
/// Decode the latest JPEG directly into a caller-owned ION DMA buffer. The
/// returned planar offsets can be passed to `/dev/cvi-vpss0` without copying
/// the decoded frame through userspace.
pub const CVI_CAMERA_IOCTL_GET_LATEST_YUV_ION: u32 = 13;

pub const CVI_CAMERA_ION_ABI_VERSION: u32 = 2;

pub const CVI_CAMERA_FRAME_NONBLOCK: u32 = 1;
pub const CVI_CAMERA_PROFILE_UVC_TOTAL: u32 = 1 << 0;
pub const CVI_CAMERA_PROFILE_VALIDATE: u32 = 1 << 1;
pub const CVI_CAMERA_PROFILE_UVC_STAGES: u32 = 1 << 2;
pub const CVI_CAMERA_PROFILE_UNKNOWN_US: u64 = u64::MAX;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraInfo {
    pub width: u16,
    pub height: u16,
    /// 1 means MJPEG.
    pub format: u8,
    pub connected: u8,
}

/// Timing contract around one accepted frame. The current sg200x-bsp API only
/// exposes the total duration of `uvc_capture_one_frame`; unavailable internal
/// stages are `CVI_CAMERA_PROFILE_UNKNOWN_US` and the
/// `CVI_CAMERA_PROFILE_UVC_STAGES` capability bit is clear. This stable ABI is
/// ready for per-stage BSP timings without changing applications later.
#[repr(C)]
#[derive(Clone, Copy, Debug, AnyBitPattern)]
pub struct CameraCaptureProfile {
    pub capabilities: u32,
    pub attempts: u32,
    pub frame_total_us: u64,
    pub uvc_total_us: u64,
    pub wait_first_packet_us: u64,
    pub usb_transfer_us: u64,
    pub jpeg_assemble_us: u64,
    pub validate_us: u64,
}

impl Default for CameraCaptureProfile {
    fn default() -> Self {
        Self {
            capabilities: (CVI_CAMERA_PROFILE_UVC_TOTAL | CVI_CAMERA_PROFILE_VALIDATE)
                & !CVI_CAMERA_PROFILE_UVC_STAGES,
            attempts: 0,
            frame_total_us: 0,
            uvc_total_us: 0,
            wait_first_packet_us: CVI_CAMERA_PROFILE_UNKNOWN_US,
            usb_transfer_us: CVI_CAMERA_PROFILE_UNKNOWN_US,
            jpeg_assemble_us: CVI_CAMERA_PROFILE_UNKNOWN_US,
            validate_us: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraCaptureStats {
    pub capture_calls: u64,
    pub transfer_attempts: u64,
    pub successful_frames: u64,
    pub failed_frames: u64,
    pub retry_attempts: u64,
    pub invalid_frames: u64,
    pub invalid_soi: u64,
    pub invalid_eoi: u64,
    pub invalid_too_small: u64,
    pub usb_errors: u64,
    pub published_frames: u64,
    pub overwritten_frames: u64,
    pub total_frame_us: u64,
    pub max_frame_us: u64,
    pub last_profile: CameraCaptureProfile,
}

/// Input/output request for the latest-frame ioctls. Set `last_sequence` to
/// the last consumed value; the blocking form waits for a strictly newer
/// frame. Set `CVI_CAMERA_FRAME_NONBLOCK` to return `EAGAIN` immediately.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, AnyBitPattern)]
pub struct CameraFrameRequest {
    pub buffer: u64,
    pub capacity: u64,
    pub last_sequence: u64,
    pub timeout_ms: u32,
    pub flags: u32,
    pub sequence: u64,
    pub timestamp_ns: u64,
    pub length: u32,
    pub width: u16,
    pub height: u16,
    pub format: u8,
    pub reserved: [u8; 3],
    pub profile: CameraCaptureProfile,
}

/// Input/output request for direct JPU decode into an ION buffer.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, AnyBitPattern)]
pub struct CameraIonFrameRequest {
    pub abi_version: u32,
    pub flags: u32,
    pub ion_fd: i32,
    pub timeout_ms: u32,
    pub buffer_offset: u64,
    pub capacity: u64,
    pub last_sequence: u64,

    pub sequence: u64,
    pub timestamp_ns: u64,
    pub y_offset: u64,
    pub cb_offset: u64,
    pub cr_offset: u64,
    pub length: u32,
    pub stride_y: u32,
    pub stride_c: u32,
    pub width: u16,
    pub height: u16,
    pub format: u8,
    pub reserved: [u8; 3],
    /// JPU decode duration for this returned frame; excludes frame wait and UVC capture.
    pub jpu_decode_us: u64,
    pub profile: CameraCaptureProfile,
}

const _: () = assert!(core::mem::size_of::<CameraIonFrameRequest>() == 168);

struct UsbCameraSession {
    cam: UvcEnumerated,
    sel: uvc::UvcStreamSelection,
}

#[derive(Default)]
struct UsbCameraState {
    session: Option<UsbCameraSession>,
    jpu: Option<JpuDecoder>,
    capture_stats: CameraCaptureStats,
}

fn jpu_dma_to_phys(v: usize) -> usize {
    virt_to_phys(VirtAddr::from(v)).as_usize()
}

pub struct CviCamera {
    state: Arc<Mutex<UsbCameraState>>,
    async_capture: Arc<AsyncCapture>,
}

struct CapturedJpeg {
    data: &'static [u8],
    profile: CameraCaptureProfile,
}

struct DecodedYuv {
    data: &'static [u8],
    width: u16,
    height: u16,
    format: u8,
}

struct DecodedIonYuv {
    width: u16,
    height: u16,
    format: u8,
    stride_y: u32,
    stride_c: u32,
    frame_size: usize,
    luma_size: usize,
    chroma_size: usize,
    decode_us: u64,
}

/// Return the planar format produced by the SG2002 JPU for this JPEG.
///
/// The JPU preserves the source JPEG's chroma sampling. In particular, a
/// 640x480 4:2:2 camera frame occupies 614400 bytes, not the 460800 bytes of
/// 4:2:0. The BSP decoder currently does not expose its parsed format, so keep
/// this small SOF parser at the ABI boundary and validate the returned size.
fn jpeg_planar_format(jpeg: &[u8]) -> Result<u8, &'static str> {
    if jpeg.len() < 4 || jpeg[0..2] != [0xff, 0xd8] {
        return Err("missing JPEG SOI");
    }

    let mut offset = 2usize;
    while offset + 1 < jpeg.len() {
        if jpeg[offset] != 0xff {
            offset += 1;
            continue;
        }
        while offset < jpeg.len() && jpeg[offset] == 0xff {
            offset += 1;
        }
        if offset >= jpeg.len() {
            break;
        }
        let marker = jpeg[offset];
        offset += 1;

        match marker {
            0x00 | 0x01 | 0xd0..=0xd9 => continue,
            0xc0 | 0xc2 => {
                if offset + 8 > jpeg.len() {
                    return Err("truncated JPEG SOF");
                }
                let segment_len = u16::from_be_bytes([jpeg[offset], jpeg[offset + 1]]) as usize;
                if segment_len < 8 || offset + segment_len > jpeg.len() {
                    return Err("invalid JPEG SOF length");
                }
                let components = jpeg[offset + 7] as usize;
                if components == 1 {
                    return Ok(CAMERA_FORMAT_YUV400);
                }
                if components != 3 || segment_len < 8 + components * 3 {
                    return Err("unsupported JPEG component count");
                }
                let y_sampling = jpeg[offset + 9];
                let cb_sampling = jpeg[offset + 12];
                let cr_sampling = jpeg[offset + 15];
                if cb_sampling != 0x11 || cr_sampling != 0x11 {
                    return Err("unsupported JPEG chroma sampling");
                }
                return match y_sampling {
                    0x22 => Ok(CAMERA_FORMAT_YUV420_PLANAR),
                    0x21 => Ok(CAMERA_FORMAT_YUV422_PLANAR),
                    0x12 => Ok(CAMERA_FORMAT_YUV440_PLANAR),
                    0x11 => Ok(CAMERA_FORMAT_YUV444_PLANAR),
                    _ => Err("unsupported JPEG luma sampling"),
                };
            }
            0xda => break,
            _ => {
                if offset + 2 > jpeg.len() {
                    return Err("truncated JPEG segment");
                }
                let segment_len = u16::from_be_bytes([jpeg[offset], jpeg[offset + 1]]) as usize;
                if segment_len < 2 || offset + segment_len > jpeg.len() {
                    return Err("invalid JPEG segment length");
                }
                offset += segment_len;
            }
        }
    }
    Err("JPEG SOF not found")
}

fn expected_planar_len(width: u32, height: u32, format: u8) -> Option<usize> {
    let (aligned_width, aligned_height) = match format {
        CAMERA_FORMAT_YUV420_PLANAR => (width.div_ceil(16) * 16, height.div_ceil(16) * 16),
        CAMERA_FORMAT_YUV422_PLANAR => (width.div_ceil(16) * 16, height.div_ceil(8) * 8),
        CAMERA_FORMAT_YUV440_PLANAR => (width.div_ceil(8) * 8, height.div_ceil(16) * 16),
        CAMERA_FORMAT_YUV444_PLANAR | CAMERA_FORMAT_YUV400 => {
            (width.div_ceil(8) * 8, height.div_ceil(8) * 8)
        }
        _ => return None,
    };
    let pixels = (aligned_width as usize).checked_mul(aligned_height as usize)?;
    match format {
        CAMERA_FORMAT_YUV420_PLANAR => pixels.checked_mul(3)?.checked_div(2),
        CAMERA_FORMAT_YUV422_PLANAR | CAMERA_FORMAT_YUV440_PLANAR => pixels.checked_mul(2),
        CAMERA_FORMAT_YUV444_PLANAR => pixels.checked_mul(3),
        CAMERA_FORMAT_YUV400 => Some(pixels),
        _ => None,
    }
}

fn expected_nv12_len(width: u32, height: u32) -> Option<usize> {
    let aligned_width = width.div_ceil(16) * 16;
    let aligned_height = height.div_ceil(16) * 16;
    (aligned_width as usize)
        .checked_mul(aligned_height as usize)?
        .checked_mul(3)?
        .checked_div(2)
}

#[derive(Default)]
struct LatestFrameStore {
    buffers: [Vec<u8>; 2],
    published_index: usize,
    sequence: u64,
    timestamp_ns: u64,
    width: u16,
    height: u16,
    profile: CameraCaptureProfile,
}

impl LatestFrameStore {
    fn publish(&mut self, frame: &CapturedJpeg, width: u16, height: u16) -> u64 {
        let next_index = self.published_index ^ 1;
        let next = &mut self.buffers[next_index];
        next.clear();
        next.extend_from_slice(frame.data);

        self.published_index = next_index;
        self.sequence = self.sequence.wrapping_add(1).max(1);
        self.timestamp_ns = monotonic_time_nanos();
        self.width = width;
        self.height = height;
        self.profile = frame.profile;
        self.sequence
    }

    fn current(&self) -> Option<&[u8]> {
        (self.sequence != 0).then(|| self.buffers[self.published_index].as_slice())
    }
}

struct AsyncCapture {
    latest: Mutex<LatestFrameStore>,
    wait_queue: WaitQueue,
    started: AtomicBool,
    stop_requested: AtomicBool,
    latest_sequence: AtomicU64,
    published_frames: AtomicU64,
    overwritten_frames: AtomicU64,
    last_consumed_sequence: AtomicU64,
}

impl AsyncCapture {
    fn new() -> Self {
        Self {
            latest: Mutex::new(LatestFrameStore::default()),
            wait_queue: WaitQueue::new(),
            started: AtomicBool::new(false),
            stop_requested: AtomicBool::new(false),
            latest_sequence: AtomicU64::new(0),
            published_frames: AtomicU64::new(0),
            overwritten_frames: AtomicU64::new(0),
            last_consumed_sequence: AtomicU64::new(0),
        }
    }

    fn clear_latest(&self) {
        *self.latest.lock() = LatestFrameStore::default();
        self.latest_sequence.store(0, Ordering::Release);
        self.last_consumed_sequence.store(0, Ordering::Release);
    }

    fn mark_consumed(&self, sequence: u64) {
        self.last_consumed_sequence
            .fetch_max(sequence, Ordering::AcqRel);
        // The producer keeps at most one prefetched frame in flight. Wake it
        // only after a consumer has finished with the published buffer.
        self.wait_queue.notify_all(true);
    }
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

/// Profiling seam for the BSP UVC implementation. When sg200x-bsp exposes
/// packet-wait, transfer and assembly timings, this wrapper is the only call
/// site that needs to change; the ioctl ABI already carries those fields.
fn uvc_capture_one_frame_profiled(
    dev: u32,
    ep0: u32,
    selection: &uvc::UvcStreamSelection,
    profile: &mut CameraCaptureProfile,
) -> Result<usize, UsbError> {
    let start = monotonic_time_nanos();
    let result = uvc::uvc_capture_one_frame(dev, ep0, selection);
    profile.uvc_total_us = (monotonic_time_nanos() - start) / 1_000;
    result
}

fn capture_frame(
    session: &UsbCameraSession,
    stats: &mut CameraCaptureStats,
) -> Result<CapturedJpeg, &'static str> {
    let dev = u32::from(session.cam.addr);
    let ep0 = session.cam.ep0_mps;
    let frame_start = monotonic_time_nanos();
    stats.capture_calls = stats.capture_calls.saturating_add(1);
    let mut last_n = 0;
    let mut last_msg = None;
    let mut last_profile = None;
    for attempt in 0..MAX_CAPTURE_TRIES {
        stats.transfer_attempts = stats.transfer_attempts.saturating_add(1);
        if attempt > 0 {
            stats.retry_attempts = stats.retry_attempts.saturating_add(1);
        }
        let mut profile = CameraCaptureProfile::default();
        let n = match uvc_capture_one_frame_profiled(dev, ep0, &session.sel, &mut profile) {
            Ok(n) => n,
            Err(e) => {
                stats.usb_errors = stats.usb_errors.saturating_add(1);
                stats.failed_frames = stats.failed_frames.saturating_add(1);
                profile.attempts = attempt + 1;
                profile.frame_total_us = (monotonic_time_nanos() - frame_start) / 1_000;
                stats.last_profile = profile;
                stats.total_frame_us = stats.total_frame_us.saturating_add(profile.frame_total_us);
                stats.max_frame_us = stats.max_frame_us.max(profile.frame_total_us);
                warn!("cvi-camera: capture failed: {e:?}");
                trace_camera_capture_frame(false, attempt + 1, 0);
                return Err("frame capture failed");
            }
        };
        last_n = n;
        let Some(frame) = dwc2_ep0::dma_rx_slice(uvc::UVC_ASSEMBLED_JPEG_DMA_OFF, n) else {
            stats.failed_frames = stats.failed_frames.saturating_add(1);
            profile.attempts = attempt + 1;
            profile.frame_total_us = (monotonic_time_nanos() - frame_start) / 1_000;
            stats.last_profile = profile;
            stats.total_frame_us = stats.total_frame_us.saturating_add(profile.frame_total_us);
            stats.max_frame_us = stats.max_frame_us.max(profile.frame_total_us);
            trace_camera_capture_frame(false, attempt + 1, n as u32);
            return Err("DMA slice out of bounds");
        };
        let validate_start = monotonic_time_nanos();
        let starts_jpeg = n >= 2 && frame[0] == 0xff && frame[1] == 0xd8;
        let ends_jpeg = n >= 2 && frame[n - 2] == 0xff && frame[n - 1] == 0xd9;
        profile.validate_us = (monotonic_time_nanos() - validate_start) / 1_000;
        last_profile = Some(profile);
        if starts_jpeg && ends_jpeg && n >= MIN_VALID_JPEG_BYTES {
            profile.attempts = attempt + 1;
            profile.frame_total_us = (monotonic_time_nanos() - frame_start) / 1_000;
            stats.successful_frames = stats.successful_frames.saturating_add(1);
            stats.total_frame_us = stats.total_frame_us.saturating_add(profile.frame_total_us);
            stats.max_frame_us = stats.max_frame_us.max(profile.frame_total_us);
            stats.last_profile = profile;
            trace_camera_capture_frame(true, attempt + 1, n as u32);
            return Ok(CapturedJpeg {
                data: frame,
                profile,
            });
        }
        stats.invalid_frames = stats.invalid_frames.saturating_add(1);
        last_msg = Some(if !starts_jpeg {
            stats.invalid_soi = stats.invalid_soi.saturating_add(1);
            "first bytes are not ff d8"
        } else if !ends_jpeg {
            stats.invalid_eoi = stats.invalid_eoi.saturating_add(1);
            "last bytes are not ff d9 (truncated)"
        } else {
            stats.invalid_too_small = stats.invalid_too_small.saturating_add(1);
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
    let mut profile = last_profile.unwrap_or_default();
    profile.attempts = MAX_CAPTURE_TRIES;
    profile.frame_total_us = (monotonic_time_nanos() - frame_start) / 1_000;
    stats.failed_frames = stats.failed_frames.saturating_add(1);
    stats.total_frame_us = stats.total_frame_us.saturating_add(profile.frame_total_us);
    stats.max_frame_us = stats.max_frame_us.max(profile.frame_total_us);
    stats.last_profile = profile;
    trace_camera_capture_frame(false, MAX_CAPTURE_TRIES, last_n as u32);
    Err("no complete JPEG after capture retries")
}

impl UsbCameraState {
    /// Clear the USB camera session and JPU state so the next
    /// `ensure_initialized` call performs a full hardware re-init.
    fn reset(&mut self) {
        self.session = None;
        self.jpu = None;
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

    fn frame(&mut self) -> VfsResult<CapturedJpeg> {
        self.ensure_initialized()?;
        capture_frame(
            self.session.as_ref().ok_or(AxError::BadState)?,
            &mut self.capture_stats,
        )
        .map_err(|msg| {
            warn!("cvi-camera: capture failed: {msg}");
            AxError::Io
        })
    }

    fn ensure_jpu(&mut self) -> VfsResult<&mut JpuDecoder> {
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

    fn decode_jpeg(&mut self, jpeg: &[u8], output: DecodeOutput) -> VfsResult<DecodedYuv> {
        let source_format = jpeg_planar_format(jpeg).map_err(|error| {
            warn!("cvi-camera: JPEG sampling parse failed: {error}");
            AxError::InvalidInput
        })?;
        if output == DecodeOutput::Nv12 && source_format != CAMERA_FORMAT_YUV420_PLANAR {
            warn!(
                "cvi-camera: NV12 requires a 4:2:0 JPEG; source format={} would produce NV16 or \
                 another layout",
                source_format
            );
            return Err(AxError::InvalidInput);
        }
        let decode_start = monotonic_time_nanos();
        let result = match output {
            DecodeOutput::Planar => self.ensure_jpu()?.decode(jpeg),
            DecodeOutput::Nv12 => self.ensure_jpu()?.decode_nv12(jpeg),
        }
        .map_err(|e| {
            warn!(
                "cvi-camera: JPU decode failed ({e}) jpeg_bytes={}, resetting JPU",
                jpeg.len()
            );
            self.jpu = None;
            AxError::Io
        })?;
        let decode_us = (monotonic_time_nanos() - decode_start) / 1_000;
        let (format, expected_len) = match output {
            DecodeOutput::Planar => (
                source_format,
                expected_planar_len(result.width, result.height, source_format)
                    .ok_or(AxError::InvalidInput)?,
            ),
            DecodeOutput::Nv12 => (
                CAMERA_FORMAT_NV12,
                expected_nv12_len(result.width, result.height).ok_or(AxError::InvalidInput)?,
            ),
        };
        if result.yuv_data.len() != expected_len {
            warn!(
                "cvi-camera: JPU output size mismatch format={} actual={} expected={}",
                format,
                result.yuv_data.len(),
                expected_len
            );
            return Err(AxError::Io);
        }
        debug!(
            "cvi-camera: JPU decode OK {}x{} format={} yuv={} bytes decode_us={}",
            result.width,
            result.height,
            format,
            result.yuv_data.len(),
            decode_us
        );
        let width = u16::try_from(result.width).map_err(|_| AxError::InvalidInput)?;
        let height = u16::try_from(result.height).map_err(|_| AxError::InvalidInput)?;
        Ok(DecodedYuv {
            data: result.yuv_data,
            width,
            height,
            format,
        })
    }

    fn decode_jpeg_into_ion(
        &mut self,
        jpeg: &[u8],
        frame_cpu: &mut [u8],
        frame_dma: usize,
    ) -> VfsResult<DecodedIonYuv> {
        let source_format = jpeg_planar_format(jpeg).map_err(|error| {
            warn!("cvi-camera: JPEG sampling parse failed: {error}");
            AxError::InvalidInput
        })?;
        let decode_start = monotonic_time_nanos();
        let result = self
            .ensure_jpu()?
            .decode_planar_into(jpeg, frame_cpu, frame_dma)
            .map_err(|error| {
                if matches!(
                    error,
                    "External JPU frame buffer is too small"
                        | "JPU DMA address overflow"
                        | "JPU DMA buffer exceeds 32-bit address range"
                ) {
                    warn!("cvi-camera: invalid external JPU buffer: {error}");
                    AxError::InvalidInput
                } else {
                    warn!(
                        "cvi-camera: JPU ION decode failed ({error}) jpeg_bytes={}, resetting JPU",
                        jpeg.len()
                    );
                    self.jpu = None;
                    AxError::Io
                }
            })?;
        let expected_len = expected_planar_len(result.width, result.height, source_format)
            .ok_or(AxError::InvalidInput)?;
        if result.frame_size != expected_len {
            warn!(
                "cvi-camera: JPU ION output size mismatch format={} actual={} expected={}",
                source_format, result.frame_size, expected_len
            );
            return Err(AxError::Io);
        }
        let decode_us = (monotonic_time_nanos() - decode_start) / 1_000;
        debug!(
            "cvi-camera: JPU ION decode OK {}x{} format={} bytes={} dma={:#x} decode_us={}",
            result.width, result.height, source_format, result.frame_size, frame_dma, decode_us
        );
        Ok(DecodedIonYuv {
            width: u16::try_from(result.width).map_err(|_| AxError::InvalidInput)?,
            height: u16::try_from(result.height).map_err(|_| AxError::InvalidInput)?,
            format: source_format,
            stride_y: result.stride_y,
            stride_c: result.stride_c,
            frame_size: result.frame_size,
            luma_size: result.luma_size,
            chroma_size: result.chroma_size,
            decode_us,
        })
    }

    fn yuv_frame(&mut self) -> VfsResult<DecodedYuv> {
        let jpeg = self.frame()?;
        self.decode_jpeg(jpeg.data, DecodeOutput::Planar)
    }
}

impl CviCamera {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(UsbCameraState::default())),
            async_capture: Arc::new(AsyncCapture::new()),
        }
    }

    fn start_async_capture(&self) -> VfsResult<()> {
        self.state.lock().ensure_initialized()?;
        if self
            .async_capture
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return if self.async_capture.stop_requested.load(Ordering::Acquire) {
                Err(AxError::WouldBlock)
            } else {
                Ok(())
            };
        }

        self.async_capture
            .stop_requested
            .store(false, Ordering::Release);
        let state = self.state.clone();
        let async_capture = self.async_capture.clone();
        ax_task::spawn_with_name(
            move || async_capture_worker(state, async_capture),
            String::from("cvi-camera-capture"),
        );
        Ok(())
    }

    fn stop_async_capture(&self) -> VfsResult<()> {
        if !self.async_capture.started.load(Ordering::Acquire) {
            return Ok(());
        }
        self.async_capture
            .stop_requested
            .store(true, Ordering::Release);
        self.async_capture.wait_queue.notify_all(true);
        for _ in 0..ASYNC_STOP_WAIT_TRIES {
            if !self.async_capture.started.load(Ordering::Acquire) {
                return Ok(());
            }
            ax_task::sleep(ASYNC_STOP_WAIT_INTERVAL);
        }
        Err(AxError::TimedOut)
    }

    fn wait_for_latest(&self, request: &CameraFrameRequest) -> VfsResult<()> {
        self.wait_for_sequence(request.last_sequence, request.timeout_ms, request.flags)
    }

    fn wait_for_sequence(&self, last_sequence: u64, timeout_ms: u32, flags: u32) -> VfsResult<()> {
        let has_new_frame =
            || self.async_capture.latest_sequence.load(Ordering::Acquire) > last_sequence;
        if has_new_frame() {
            return Ok(());
        }
        if flags & CVI_CAMERA_FRAME_NONBLOCK != 0 || timeout_ms == 0 {
            return Err(AxError::WouldBlock);
        }
        if !self.async_capture.started.load(Ordering::Acquire) {
            return Err(AxError::BadState);
        }

        let timed_out = self
            .async_capture
            .wait_queue
            .wait_timeout_until(Duration::from_millis(u64::from(timeout_ms)), || {
                has_new_frame() || !self.async_capture.started.load(Ordering::Acquire)
            });
        if timed_out {
            return Err(AxError::TimedOut);
        }
        if has_new_frame() {
            Ok(())
        } else {
            Err(AxError::BadState)
        }
    }

    fn latest_mjpeg(&self, arg: usize) -> VfsResult<usize> {
        let mut request: CameraFrameRequest = (arg as *const CameraFrameRequest).vm_read()?;
        self.wait_for_latest(&request)?;

        let latest = self.async_capture.latest.lock();
        let frame = latest.current().ok_or(AxError::WouldBlock)?;
        request.sequence = latest.sequence;
        request.timestamp_ns = latest.timestamp_ns;
        request.length = u32::try_from(frame.len()).map_err(|_| AxError::InvalidInput)?;
        request.width = latest.width;
        request.height = latest.height;
        request.format = CAMERA_FORMAT_MJPEG;
        request.profile = latest.profile;

        if request.capacity < frame.len() as u64 {
            (arg as *mut CameraFrameRequest).vm_write(request)?;
            return Err(AxError::InvalidInput);
        }
        vm_write_slice(request.buffer as *mut u8, frame)?;
        drop(latest);
        (arg as *mut CameraFrameRequest).vm_write(request)?;
        self.async_capture.mark_consumed(request.sequence);
        Ok(request.length as usize)
    }

    fn latest_yuv(&self, arg: usize, output: DecodeOutput) -> VfsResult<usize> {
        let mut request: CameraFrameRequest = (arg as *const CameraFrameRequest).vm_read()?;
        let mut decode_attempt = 0u32;
        let (decoded, sequence, timestamp_ns, profile) = loop {
            self.wait_for_latest(&request)?;

            // Copy the compressed frame before taking the camera/JPU state
            // lock. The producer lock order is state -> latest; releasing
            // latest here prevents an ABBA deadlock and the copy is much
            // smaller than YUV.
            let (jpeg, sequence, timestamp_ns, profile) = {
                let latest = self.async_capture.latest.lock();
                (
                    latest.current().ok_or(AxError::WouldBlock)?.to_vec(),
                    latest.sequence,
                    latest.timestamp_ns,
                    latest.profile,
                )
            };
            decode_attempt += 1;
            match self.state.lock().decode_jpeg(&jpeg, output) {
                Ok(decoded) => break (decoded, sequence, timestamp_ns, profile),
                Err(AxError::Io) if decode_attempt < MAX_JPU_DECODE_TRIES => {
                    self.async_capture.mark_consumed(sequence);
                    warn!(
                        "cvi-camera: dropping undecodable sequence={} and waiting for a newer \
                         frame (attempt {}/{})",
                        sequence, decode_attempt, MAX_JPU_DECODE_TRIES
                    );
                    request.last_sequence = sequence;
                }
                Err(error) => return Err(error),
            }
        };
        request.sequence = sequence;
        request.timestamp_ns = timestamp_ns;
        request.length = u32::try_from(decoded.data.len()).map_err(|_| AxError::InvalidInput)?;
        request.width = decoded.width;
        request.height = decoded.height;
        request.format = decoded.format;
        request.profile = profile;

        if request.capacity < decoded.data.len() as u64 {
            (arg as *mut CameraFrameRequest).vm_write(request)?;
            return Err(AxError::InvalidInput);
        }
        vm_write_slice(request.buffer as *mut u8, decoded.data)?;
        (arg as *mut CameraFrameRequest).vm_write(request)?;
        self.async_capture.mark_consumed(sequence);
        Ok(request.length as usize)
    }

    fn latest_yuv_to_ion(&self, arg: usize) -> VfsResult<usize> {
        let mut request: CameraIonFrameRequest = (arg as *const CameraIonFrameRequest).vm_read()?;
        if request.abi_version != CVI_CAMERA_ION_ABI_VERSION
            || request.flags & !CVI_CAMERA_FRAME_NONBLOCK != 0
            || request.capacity == 0
        {
            return Err(AxError::InvalidInput);
        }
        let buffer = lookup_ion_buffer(request.ion_fd)?;
        let buffer_end = request
            .buffer_offset
            .checked_add(request.capacity)
            .ok_or(AxError::InvalidInput)?;
        if buffer_end > buffer.size as u64 {
            return Err(AxError::InvalidInput);
        }
        let buffer_offset =
            usize::try_from(request.buffer_offset).map_err(|_| AxError::InvalidInput)?;
        let capacity = usize::try_from(request.capacity).map_err(|_| AxError::InvalidInput)?;
        let frame_dma = buffer
            .dma_info
            .bus_addr
            .as_u64()
            .checked_add(request.buffer_offset)
            .and_then(|address| usize::try_from(address).ok())
            .ok_or(AxError::InvalidInput)?;
        let frame_cpu = unsafe {
            // SAFETY: `buffer` keeps the coherent allocation alive for the
            // complete blocking decode. The range was checked above. The ABI
            // requires userspace not to access this ION range until ioctl 13
            // returns, giving JPU exclusive DMA ownership during the call.
            core::slice::from_raw_parts_mut(
                buffer.dma_info.cpu_addr.as_ptr().add(buffer_offset),
                capacity,
            )
        };

        let mut decode_attempt = 0u32;
        let (decoded, sequence, timestamp_ns, profile) = loop {
            self.wait_for_sequence(request.last_sequence, request.timeout_ms, request.flags)?;
            let (jpeg, sequence, timestamp_ns, profile) = {
                let latest = self.async_capture.latest.lock();
                (
                    latest.current().ok_or(AxError::WouldBlock)?.to_vec(),
                    latest.sequence,
                    latest.timestamp_ns,
                    latest.profile,
                )
            };
            decode_attempt += 1;
            match self
                .state
                .lock()
                .decode_jpeg_into_ion(&jpeg, frame_cpu, frame_dma)
            {
                Ok(decoded) => break (decoded, sequence, timestamp_ns, profile),
                Err(AxError::Io) if decode_attempt < MAX_JPU_DECODE_TRIES => {
                    self.async_capture.mark_consumed(sequence);
                    warn!(
                        "cvi-camera: dropping undecodable ION sequence={} and waiting for a newer \
                         frame (attempt {}/{})",
                        sequence, decode_attempt, MAX_JPU_DECODE_TRIES
                    );
                    request.last_sequence = sequence;
                }
                Err(error) => return Err(error),
            }
        };

        request.sequence = sequence;
        request.timestamp_ns = timestamp_ns;
        request.y_offset = request.buffer_offset;
        request.cb_offset = request
            .y_offset
            .checked_add(decoded.luma_size as u64)
            .ok_or(AxError::InvalidInput)?;
        request.cr_offset = request
            .cb_offset
            .checked_add(decoded.chroma_size as u64)
            .ok_or(AxError::InvalidInput)?;
        request.length = u32::try_from(decoded.frame_size).map_err(|_| AxError::InvalidInput)?;
        request.stride_y = decoded.stride_y;
        request.stride_c = decoded.stride_c;
        request.width = decoded.width;
        request.height = decoded.height;
        request.format = decoded.format;
        request.jpu_decode_us = decoded.decode_us;
        request.profile = profile;
        (arg as *mut CameraIonFrameRequest).vm_write(request)?;
        self.async_capture.mark_consumed(sequence);
        Ok(decoded.frame_size)
    }

    fn capture_stats(&self) -> CameraCaptureStats {
        let mut stats = self.state.lock().capture_stats;
        stats.published_frames = self.async_capture.published_frames.load(Ordering::Acquire);
        stats.overwritten_frames = self
            .async_capture
            .overwritten_frames
            .load(Ordering::Acquire);
        stats
    }

    fn reset_capture_stats(&self) {
        self.state.lock().capture_stats = CameraCaptureStats::default();
        self.async_capture
            .published_frames
            .store(0, Ordering::Release);
        self.async_capture
            .overwritten_frames
            .store(0, Ordering::Release);
    }
}

fn async_capture_worker(state: Arc<Mutex<UsbCameraState>>, async_capture: Arc<AsyncCapture>) {
    while !async_capture.stop_requested.load(Ordering::Acquire) {
        // The SG2002 target has one application-class C906 and uses the RR
        // scheduler, whose set_priority implementation is a no-op. Unlimited
        // latest-frame polling therefore steals CPU from preprocessing and
        // TPU submission just to overwrite frames that cannot be consumed.
        // Keep one frame prefetched, then sleep until it is consumed.
        async_capture.wait_queue.wait_until(|| {
            let published = async_capture.latest_sequence.load(Ordering::Acquire);
            async_capture.stop_requested.load(Ordering::Acquire)
                || published == 0
                || async_capture.last_consumed_sequence.load(Ordering::Acquire) >= published
        });
        if async_capture.stop_requested.load(Ordering::Acquire) {
            break;
        }

        // The DMA JPEG slice remains valid only until the next UVC capture.
        // Keep the camera lock while copying it into the inactive latest-frame
        // buffer, then publish by swapping the active index under one lock.
        let published = {
            let mut camera = state.lock();
            match camera.frame() {
                Ok(frame) => {
                    let (width, height) = camera
                        .session
                        .as_ref()
                        .map(|session| (session.sel.frame_w, session.sel.frame_h))
                        .unwrap_or_default();
                    let mut latest = async_capture.latest.lock();
                    if latest.sequence != 0
                        && async_capture.last_consumed_sequence.load(Ordering::Acquire)
                            < latest.sequence
                    {
                        async_capture
                            .overwritten_frames
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    let sequence = latest.publish(&frame, width, height);
                    async_capture
                        .latest_sequence
                        .store(sequence, Ordering::Release);
                    true
                }
                Err(_) => false,
            }
        };

        if published {
            async_capture
                .published_frames
                .fetch_add(1, Ordering::Relaxed);
            async_capture.wait_queue.notify_all(true);
        } else {
            ax_task::sleep(ASYNC_ERROR_BACKOFF);
        }
    }

    async_capture.started.store(false, Ordering::Release);
    async_capture.wait_queue.notify_all(true);
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
            CVI_CAMERA_IOCTL_START_ASYNC => "START_ASYNC",
            CVI_CAMERA_IOCTL_STOP_ASYNC => "STOP_ASYNC",
            CVI_CAMERA_IOCTL_GET_LATEST_FRAME => "GET_LATEST_FRAME",
            CVI_CAMERA_IOCTL_GET_CAPTURE_STATS => "GET_CAPTURE_STATS",
            CVI_CAMERA_IOCTL_RESET_CAPTURE_STATS => "RESET_CAPTURE_STATS",
            CVI_CAMERA_IOCTL_GET_LATEST_YUV_FRAME => "GET_LATEST_YUV_FRAME",
            CVI_CAMERA_IOCTL_GET_LATEST_NV12_FRAME => "GET_LATEST_NV12_FRAME",
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
        // A capture may still be inside the BSP's blocking UVC call. Request
        // shutdown without tearing its session out from underneath it; INIT or
        // HARD_RESET performs the synchronized reset on the next open.
        self.async_capture
            .stop_requested
            .store(true, Ordering::Release);
        self.async_capture.wait_queue.notify_all(true);
    }

    fn ioctl(&self, cmd: u32, arg: usize) -> VfsResult<usize> {
        let start = monotonic_time_nanos();
        let result = match cmd {
            CVI_CAMERA_IOCTL_INIT => {
                // Clear any stale session so full hardware init runs again.
                self.stop_async_capture()?;
                self.state.lock().reset();
                self.async_capture.clear_latest();
                self.state.lock().ensure_initialized().map(|_| 0)
            }
            CVI_CAMERA_IOCTL_HARD_RESET => {
                // VBUS power-cycle: physically power-cycle the camera
                // module, then do a full hardware init from scratch.
                info!("cvi-camera: HARD_RESET ioctl — power-cycling VBUS ...");
                self.stop_async_capture()?;
                self.state.lock().reset();
                self.async_capture.clear_latest();
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
                vm_write_slice(arg as *mut u8, frame.data)?;
                Ok(frame.data.len())
            }
            CVI_CAMERA_IOCTL_GET_YUV_FRAME => {
                let yuv = self.state.lock().yuv_frame()?;
                vm_write_slice(arg as *mut u8, yuv.data)?;
                Ok(yuv.data.len())
            }
            CVI_CAMERA_IOCTL_START_ASYNC => self.start_async_capture().map(|_| 0),
            CVI_CAMERA_IOCTL_STOP_ASYNC => self.stop_async_capture().map(|_| 0),
            CVI_CAMERA_IOCTL_GET_LATEST_FRAME => self.latest_mjpeg(arg),
            CVI_CAMERA_IOCTL_GET_CAPTURE_STATS => {
                (arg as *mut CameraCaptureStats).vm_write(self.capture_stats())?;
                Ok(0)
            }
            CVI_CAMERA_IOCTL_RESET_CAPTURE_STATS => {
                self.reset_capture_stats();
                Ok(0)
            }
            CVI_CAMERA_IOCTL_GET_LATEST_YUV_FRAME => self.latest_yuv(arg, DecodeOutput::Planar),
            CVI_CAMERA_IOCTL_GET_LATEST_NV12_FRAME => self.latest_yuv(arg, DecodeOutput::Nv12),
            CVI_CAMERA_IOCTL_GET_LATEST_YUV_ION => self.latest_yuv_to_ion(arg),
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

fn lookup_ion_buffer(file_descriptor: i32) -> VfsResult<Arc<IonBuffer>> {
    let file = get_file_like(file_descriptor).map_err(|_| AxError::BadFileDescriptor)?;
    let ion_file: Arc<IonBufferFile> = file
        .downcast_arc::<IonBufferFile>()
        .map_err(|_| AxError::InvalidInput)?;
    Ok(ion_file.buffer().clone())
}

#[cfg(test)]
mod tests {
    use super::{
        CAMERA_FORMAT_YUV400, CAMERA_FORMAT_YUV420_PLANAR, CAMERA_FORMAT_YUV422_PLANAR,
        CAMERA_FORMAT_YUV440_PLANAR, CAMERA_FORMAT_YUV444_PLANAR, expected_nv12_len,
        expected_planar_len, jpeg_planar_format,
    };

    fn three_component_jpeg(y_sampling: u8) -> alloc::vec::Vec<u8> {
        alloc::vec![
            0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x01, 0xe0, 0x02, 0x80, 0x03, 0x01,
            y_sampling, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xff, 0xd9,
        ]
    }

    #[test]
    fn jpeg_sampling_maps_to_camera_abi_formats() {
        assert_eq!(
            jpeg_planar_format(&three_component_jpeg(0x22)),
            Ok(CAMERA_FORMAT_YUV420_PLANAR)
        );
        assert_eq!(
            jpeg_planar_format(&three_component_jpeg(0x21)),
            Ok(CAMERA_FORMAT_YUV422_PLANAR)
        );
        assert_eq!(
            jpeg_planar_format(&three_component_jpeg(0x12)),
            Ok(CAMERA_FORMAT_YUV440_PLANAR)
        );
        assert_eq!(
            jpeg_planar_format(&three_component_jpeg(0x11)),
            Ok(CAMERA_FORMAT_YUV444_PLANAR)
        );
    }

    #[test]
    fn expected_lengths_match_aligned_jpu_layout() {
        assert_eq!(
            expected_planar_len(640, 480, CAMERA_FORMAT_YUV420_PLANAR),
            Some(460_800)
        );
        assert_eq!(
            expected_planar_len(640, 480, CAMERA_FORMAT_YUV422_PLANAR),
            Some(614_400)
        );
        assert_eq!(
            expected_planar_len(640, 480, CAMERA_FORMAT_YUV440_PLANAR),
            Some(614_400)
        );
        assert_eq!(
            expected_planar_len(640, 480, CAMERA_FORMAT_YUV444_PLANAR),
            Some(921_600)
        );
        assert_eq!(
            expected_planar_len(640, 480, CAMERA_FORMAT_YUV400),
            Some(307_200)
        );
        assert_eq!(expected_nv12_len(640, 480), Some(460_800));
        assert_eq!(expected_nv12_len(641, 481), Some(488_064));
    }
}
