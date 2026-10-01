//! StarryOS integration for the CV181x VPSS offline scaler path.

use alloc::sync::Arc;
use core::{
    ptr::NonNull,
    sync::atomic::{AtomicU64, Ordering, fence},
    time::Duration,
};

use ax_errno::{AxError, AxResult};
use ax_memory_addr::PhysAddr;
use ax_sync::Mutex;
use ax_task::WaitQueue;
use axfs_ng_vfs::VfsResult;
use sg200x_bsp::soc::CLKGEN_BASE;
use sg2002_tpu::ion::IonBuffer;
use sg2002_vpss::{
    CompletionState, DestinationFrame, Error as VpssError, Job, JobCompletion, MmioRegion,
    Nv12Frame, Plane, Rect, RgbColor, RgbPlanarFrame, Size, SourceFrame, VpssControl,
    Yuv422PlanarFrame,
    types::{MAX_DIMENSION, MIN_DIMENSION, STRIDE_ALIGNMENT},
};
use starry_vm::{VmMutPtr, VmPtr};

use super::uapi::{
    VPSS_ABI_VERSION, VPSS_FEATURE_BORDER, VPSS_FEATURE_BT601_CSC, VPSS_FEATURE_ION_FD,
    VPSS_FEATURE_IRQ, VPSS_FEATURE_NV12, VPSS_FEATURE_RGB_PLANAR_OUTPUT, VPSS_FEATURE_SCALE,
    VPSS_FEATURE_YUV422P_INPUT, VPSS_IOCTL_GET_INFO, VPSS_IOCTL_GET_STATS, VPSS_IOCTL_RESET_STATS,
    VPSS_IOCTL_RUN, VPSS_IOCTL_RUN_YUV422P, VPSS_IOCTL_RUN_YUV422P_RGB, VPSS_STATUS_BUSY,
    VPSS_STATUS_INVALID, VPSS_STATUS_IO, VPSS_STATUS_OK, VPSS_STATUS_TIMEOUT, VpssInfo, VpssRun,
    VpssRunYuv422p, VpssRunYuv422pRgb, VpssStats,
};
use crate::{
    file::{get_file_like, ion::IonBufferFile},
    pseudofs::{
        DeviceOps,
        dev::{IrqRegistration, request_shared_disabled},
    },
};

const VPSS_COMPATIBLES: &[&str] = &["cvitek,vpss"];
const VPSS_IRQ_NAME: &str = "sc";
const VPSS_DEFAULT_TIMEOUT_MS: u32 = 100;
const VPSS_MAX_TIMEOUT_MS: u32 = 5_000;
const CLKGEN_MMIO_SIZE: usize = 0x1000;
const CLK_ENABLE_2: usize = 0x008;
const CLK_ENABLE_3: usize = 0x00c;
const CLK_ENABLE_2_MASK: u32 = (1 << 4) // AXI_VIP
    | (1 << 5) // VIP_SYS_0
    | (1 << 6) // VIP_SYS_1
    | (1 << 22) // IMG_V
    | (1 << 23) // SC_TOP
    | (1 << 25); // SC_V1
const CLK_ENABLE_3_MASK: u32 = 1 << 29; // VIP_SYS_2

static DONE_WAIT_QUEUE: WaitQueue = WaitQueue::new();

#[derive(Clone, Copy, Debug)]
struct VpssResource {
    mmio_physical: usize,
    mmio_size: usize,
    dphy_physical: usize,
    dphy_size: usize,
    irq: ax_runtime::hal::irq::IrqId,
}

impl VpssResource {
    fn probe() -> Option<Self> {
        rdrive::with_fdt(|fdt| {
            fdt.find_compatible(VPSS_COMPATIBLES)
                .into_iter()
                .find_map(Self::from_fdt_node)
        })
        .flatten()
    }

    fn from_fdt_node(node: rdrive::probe::fdt::NodeType<'_>) -> Option<Self> {
        if matches!(
            node.as_node().status(),
            Some(rdrive::probe::fdt::Status::Disabled)
        ) {
            return None;
        }
        let mut registers = node.regs().into_iter();
        let scaler = registers.next()?;
        let dphy = registers.next()?;
        let mmio_size = scaler.size? as usize;
        let dphy_size = dphy.size? as usize;
        if mmio_size < sg2002_vpss::registers::MMIO_MIN_SIZE || dphy_size < 0x100 {
            warn!(
                "[VPSS] invalid FDT windows: scaler={:#x}, dphy={:#x}",
                mmio_size, dphy_size
            );
            return None;
        }
        let binding = ax_driver::binding_irq_from_named_fdt_interrupt(&node, VPSS_IRQ_NAME)
            .ok()
            .flatten()?;
        let irq = ax_runtime::irq::resolve_binding_irq(binding).ok()?;
        Some(Self {
            mmio_physical: scaler.address as usize,
            mmio_size,
            dphy_physical: dphy.address as usize,
            dphy_size,
            irq,
        })
    }
}

#[derive(Default)]
struct RuntimeStats {
    submitted_jobs: AtomicU64,
    failed_jobs: AtomicU64,
    total_elapsed_ns: AtomicU64,
    last_elapsed_ns: AtomicU64,
    max_elapsed_ns: AtomicU64,
}

struct ExecutedJob {
    hardware_start_ns: u64,
    hardware_done_ns: u64,
    elapsed_ns: u64,
    completion: JobCompletion,
}

impl RuntimeStats {
    fn record(&self, elapsed_ns: u64, failed: bool) {
        self.submitted_jobs.fetch_add(1, Ordering::Relaxed);
        if failed {
            self.failed_jobs.fetch_add(1, Ordering::Relaxed);
        }
        self.total_elapsed_ns
            .fetch_add(elapsed_ns, Ordering::Relaxed);
        self.last_elapsed_ns.store(elapsed_ns, Ordering::Relaxed);
        self.max_elapsed_ns.fetch_max(elapsed_ns, Ordering::Relaxed);
    }

    fn reset(&self) {
        self.submitted_jobs.store(0, Ordering::Relaxed);
        self.failed_jobs.store(0, Ordering::Relaxed);
        self.total_elapsed_ns.store(0, Ordering::Relaxed);
        self.last_elapsed_ns.store(0, Ordering::Relaxed);
        self.max_elapsed_ns.store(0, Ordering::Relaxed);
    }
}

pub struct VpssDevice {
    control: Mutex<VpssControl<MmioRegion>>,
    completion: Arc<CompletionState>,
    resource: VpssResource,
    runtime_stats: RuntimeStats,
    _irq_registration: IrqRegistration,
}

impl VpssDevice {
    pub fn probe() -> Option<Self> {
        let resource = VpssResource::probe().or_else(|| {
            warn!("[VPSS] enabled cvitek,vpss node with sc IRQ not found in FDT");
            None
        })?;
        if let Err(error) = enable_vpss_clocks() {
            warn!("[VPSS] failed to enable CV181x clock gates: {error:?}");
            return None;
        }
        let virtual_address =
            axklib::mem::iomap(PhysAddr::from(resource.mmio_physical), resource.mmio_size)
                .map_err(|error| {
                    warn!(
                        "[VPSS] failed to map {:#x}+{:#x}: {error:?}",
                        resource.mmio_physical, resource.mmio_size
                    );
                })
                .ok()?;
        let base = NonNull::new(virtual_address.as_mut_ptr())?;
        // SAFETY: iomap created a permanent device mapping covering the full
        // FDT scaler window, which outlives the device and IRQ registration.
        let mmio = unsafe { MmioRegion::new(base, resource.mmio_size) }.ok()?;
        let completion = Arc::new(CompletionState::new());
        let mut control = VpssControl::new(mmio, Arc::clone(&completion));
        control.initialize();
        let handler = control.irq_handler();
        let irq = resource.irq;
        let registration = request_shared_disabled(irq, move |_| match handler.handle() {
            Some(event) => {
                if event.wake_waiter {
                    DONE_WAIT_QUEUE.notify_all_from_irq();
                }
                ax_runtime::hal::irq::IrqReturn::Handled
            }
            None => ax_runtime::hal::irq::IrqReturn::Unhandled,
        })
        .map_err(|error| {
            warn!("[VPSS] failed to register IRQ {irq:?}: {error:?}");
        })
        .ok()?;
        registration
            .enable()
            .map_err(|error| {
                warn!("[VPSS] failed to enable IRQ {irq:?}: {error:?}");
            })
            .ok()?;

        info!(
            "[VPSS] offline NV12/YUV422P scaler ready: mmio={:#x}+{:#x}, dphy={:#x}+{:#x}, \
             irq={irq:?}",
            resource.mmio_physical, resource.mmio_size, resource.dphy_physical, resource.dphy_size
        );
        Some(Self {
            control: Mutex::new(control),
            completion,
            resource,
            runtime_stats: RuntimeStats::default(),
            _irq_registration: registration,
        })
    }

    fn run_ioctl(&self, user_address: usize) -> VfsResult<usize> {
        let user_pointer = user_address as *mut VpssRun;
        let mut request = user_pointer.vm_read()?;
        request.clear_output();
        request.queue_enter_ns = now_ns();
        let result = self.run(&mut request);
        request.status = match &result {
            Ok(()) => VPSS_STATUS_OK,
            Err(error) => status_from_error(*error),
        };
        user_pointer.vm_write(request)?;
        result.map(|()| 0)
    }

    fn run_yuv422p_ioctl(&self, user_address: usize) -> VfsResult<usize> {
        let user_pointer = user_address as *mut VpssRunYuv422p;
        let mut request = user_pointer.vm_read()?;
        request.clear_output();
        request.queue_enter_ns = now_ns();
        let result = self.run_yuv422p(&mut request);
        request.status = match &result {
            Ok(()) => VPSS_STATUS_OK,
            Err(error) => status_from_error(*error),
        };
        user_pointer.vm_write(request)?;
        result.map(|()| 0)
    }

    fn run_yuv422p_rgb_ioctl(&self, user_address: usize) -> VfsResult<usize> {
        let user_pointer = user_address as *mut VpssRunYuv422pRgb;
        let mut request = user_pointer.vm_read()?;
        request.clear_output();
        request.queue_enter_ns = now_ns();
        let result = self.run_yuv422p_rgb(&mut request);
        request.status = match &result {
            Ok(()) => VPSS_STATUS_OK,
            Err(error) => status_from_error(*error),
        };
        user_pointer.vm_write(request)?;
        result.map(|()| 0)
    }

    fn run(&self, request: &mut VpssRun) -> AxResult<()> {
        if request.abi_version != VPSS_ABI_VERSION || request.flags != 0 {
            return Err(AxError::InvalidInput);
        }
        if request.source_fd == request.destination_fd {
            return Err(AxError::InvalidInput);
        }
        let source_buffer = lookup_ion_buffer(request.source_fd)?;
        let destination_buffer = lookup_ion_buffer(request.destination_fd)?;
        if source_buffer.handle == destination_buffer.handle {
            return Err(AxError::InvalidInput);
        }

        let source = resolve_nv12_frame(
            &source_buffer,
            request.source_y_offset,
            request.source_uv_offset,
            Size {
                width: request.source_width,
                height: request.source_height,
            },
            request.source_y_stride,
            request.source_uv_stride,
        )?;
        let destination = resolve_nv12_frame(
            &destination_buffer,
            request.destination_y_offset,
            request.destination_uv_offset,
            Size {
                width: request.destination_width,
                height: request.destination_height,
            },
            request.destination_y_stride,
            request.destination_uv_stride,
        )?;
        let job = Job {
            source: SourceFrame::Nv12(source),
            crop: Rect {
                x: request.crop_x,
                y: request.crop_y,
                width: request.crop_width,
                height: request.crop_height,
            },
            destination: DestinationFrame::Nv12(destination),
            content: Rect {
                x: 0,
                y: 0,
                width: request.destination_width,
                height: request.destination_height,
            },
            border_color: RgbColor::default(),
            sequence: request.sequence,
            timestamp_ns: request.timestamp_ns,
        };
        job.validate().map_err(map_driver_error)?;

        let executed = self.execute_job(job, request.timeout_ms)?;
        apply_execution_to_nv12_request(request, &executed, job.timestamp_ns);
        executed.completion.result.map_err(map_driver_error)
    }

    fn run_yuv422p(&self, request: &mut VpssRunYuv422p) -> AxResult<()> {
        if request.abi_version != VPSS_ABI_VERSION || request.flags != 0 {
            return Err(AxError::InvalidInput);
        }
        if request.source_fd == request.destination_fd {
            return Err(AxError::InvalidInput);
        }
        let source_buffer = lookup_ion_buffer(request.source_fd)?;
        let destination_buffer = lookup_ion_buffer(request.destination_fd)?;
        if source_buffer.handle == destination_buffer.handle {
            return Err(AxError::InvalidInput);
        }

        let source = resolve_yuv422_planar_frame(
            &source_buffer,
            request.source_y_offset,
            request.source_cb_offset,
            request.source_cr_offset,
            Size {
                width: request.source_width,
                height: request.source_height,
            },
            request.source_y_stride,
            request.source_c_stride,
        )?;
        let destination = resolve_nv12_frame(
            &destination_buffer,
            request.destination_y_offset,
            request.destination_uv_offset,
            Size {
                width: request.destination_width,
                height: request.destination_height,
            },
            request.destination_y_stride,
            request.destination_uv_stride,
        )?;
        let job = Job {
            source: SourceFrame::Yuv422Planar(source),
            crop: Rect {
                x: request.crop_x,
                y: request.crop_y,
                width: request.crop_width,
                height: request.crop_height,
            },
            destination: DestinationFrame::Nv12(destination),
            content: Rect {
                x: 0,
                y: 0,
                width: request.destination_width,
                height: request.destination_height,
            },
            border_color: RgbColor::default(),
            sequence: request.sequence,
            timestamp_ns: request.timestamp_ns,
        };
        job.validate().map_err(map_driver_error)?;

        let executed = self.execute_job(job, request.timeout_ms)?;
        apply_execution_to_yuv422p_request(request, &executed, job.timestamp_ns);
        executed.completion.result.map_err(map_driver_error)
    }

    fn run_yuv422p_rgb(&self, request: &mut VpssRunYuv422pRgb) -> AxResult<()> {
        if request.abi_version != VPSS_ABI_VERSION
            || request.flags != 0
            || request.border_rgb & 0xff00_0000 != 0
        {
            return Err(AxError::InvalidInput);
        }
        if request.source_fd == request.destination_fd {
            return Err(AxError::InvalidInput);
        }
        let source_buffer = lookup_ion_buffer(request.source_fd)?;
        let destination_buffer = lookup_ion_buffer(request.destination_fd)?;
        if source_buffer.handle == destination_buffer.handle {
            return Err(AxError::InvalidInput);
        }

        let source = resolve_yuv422_planar_frame(
            &source_buffer,
            request.source_y_offset,
            request.source_cb_offset,
            request.source_cr_offset,
            Size {
                width: request.source_width,
                height: request.source_height,
            },
            request.source_y_stride,
            request.source_c_stride,
        )?;
        let destination = resolve_rgb_planar_frame(
            &destination_buffer,
            request.destination_r_offset,
            request.destination_g_offset,
            request.destination_b_offset,
            Size {
                width: request.destination_width,
                height: request.destination_height,
            },
            request.destination_r_stride,
            request.destination_gb_stride,
        )?;
        let job = Job {
            source: SourceFrame::Yuv422Planar(source),
            crop: Rect {
                x: request.crop_x,
                y: request.crop_y,
                width: request.crop_width,
                height: request.crop_height,
            },
            destination: DestinationFrame::RgbPlanar(destination),
            content: Rect {
                x: request.content_x,
                y: request.content_y,
                width: request.content_width,
                height: request.content_height,
            },
            border_color: RgbColor {
                r: request.border_rgb as u8,
                g: (request.border_rgb >> 8) as u8,
                b: (request.border_rgb >> 16) as u8,
            },
            sequence: request.sequence,
            timestamp_ns: request.timestamp_ns,
        };
        job.validate().map_err(map_driver_error)?;

        let executed = self.execute_job(job, request.timeout_ms)?;
        apply_execution_to_yuv422p_rgb_request(request, &executed, job.timestamp_ns);
        executed.completion.result.map_err(map_driver_error)
    }

    fn execute_job(&self, job: Job, timeout_ms: u32) -> AxResult<ExecutedJob> {
        // This sleepable mutex serializes the single SC_V1 hardware channel.
        // The IRQ endpoint owns no part of it, so waiting here cannot deadlock
        // interrupt completion.
        let mut control = self.control.lock();
        let hardware_start_ns = now_ns();
        control.start(job).map_err(map_driver_error)?;
        let timeout_ms = match timeout_ms {
            0 => VPSS_DEFAULT_TIMEOUT_MS,
            value => value.min(VPSS_MAX_TIMEOUT_MS),
        };
        let timed_out = DONE_WAIT_QUEUE
            .wait_timeout_until(Duration::from_millis(u64::from(timeout_ms)), || {
                self.completion.is_finished()
            });
        let hardware_done_ns = now_ns();
        let completion = if timed_out && !self.completion.is_finished() {
            control.recover_timeout().map_err(map_driver_error)?
        } else {
            control.finish().map_err(map_driver_error)?
        };

        let elapsed_ns = hardware_done_ns.saturating_sub(hardware_start_ns);
        let failed = completion.result.is_err();
        self.runtime_stats.record(elapsed_ns, failed);
        Ok(ExecutedJob {
            hardware_start_ns,
            hardware_done_ns,
            elapsed_ns,
            completion,
        })
    }

    fn info(&self) -> VpssInfo {
        VpssInfo {
            abi_version: VPSS_ABI_VERSION,
            features: VPSS_FEATURE_NV12
                | VPSS_FEATURE_SCALE
                | VPSS_FEATURE_BT601_CSC
                | VPSS_FEATURE_ION_FD
                | VPSS_FEATURE_IRQ
                | VPSS_FEATURE_YUV422P_INPUT
                | VPSS_FEATURE_RGB_PLANAR_OUTPUT
                | VPSS_FEATURE_BORDER,
            min_dimension: MIN_DIMENSION,
            max_dimension: MAX_DIMENSION,
            stride_alignment: STRIDE_ALIGNMENT,
            reserved0: 0,
            mmio_physical: self.resource.mmio_physical as u64,
            mmio_size: self.resource.mmio_size as u64,
            irq_domain: u32::from(self.resource.irq.domain.0),
            irq_hwirq: self.resource.irq.hwirq.0,
            reserved: [0; 2],
        }
    }

    fn stats(&self) -> VpssStats {
        let irq = self.completion.stats();
        VpssStats {
            irq_count: irq.irq_count,
            completed_jobs: irq.completed_jobs,
            program_late_errors: irq.program_late_errors,
            timeout_errors: irq.timeout_errors,
            spurious_irqs: irq.spurious_irqs,
            last_irq_status: irq.last_irq_status,
            reserved0: 0,
            submitted_jobs: self.runtime_stats.submitted_jobs.load(Ordering::Relaxed),
            failed_jobs: self.runtime_stats.failed_jobs.load(Ordering::Relaxed),
            total_elapsed_ns: self.runtime_stats.total_elapsed_ns.load(Ordering::Relaxed),
            last_elapsed_ns: self.runtime_stats.last_elapsed_ns.load(Ordering::Relaxed),
            max_elapsed_ns: self.runtime_stats.max_elapsed_ns.load(Ordering::Relaxed),
        }
    }
}

impl DeviceOps for VpssDevice {
    fn read_at(&self, _buffer: &mut [u8], _offset: u64) -> VfsResult<usize> {
        Ok(0)
    }

    fn write_at(&self, _buffer: &[u8], _offset: u64) -> VfsResult<usize> {
        Ok(0)
    }

    fn ioctl(&self, command: u32, argument: usize) -> VfsResult<usize> {
        match command {
            VPSS_IOCTL_RUN => self.run_ioctl(argument),
            VPSS_IOCTL_RUN_YUV422P => self.run_yuv422p_ioctl(argument),
            VPSS_IOCTL_RUN_YUV422P_RGB => self.run_yuv422p_rgb_ioctl(argument),
            VPSS_IOCTL_GET_INFO => {
                (argument as *mut VpssInfo).vm_write(self.info())?;
                Ok(0)
            }
            VPSS_IOCTL_GET_STATS => {
                (argument as *mut VpssStats).vm_write(self.stats())?;
                Ok(0)
            }
            VPSS_IOCTL_RESET_STATS => {
                self.completion.reset_stats();
                self.runtime_stats.reset();
                Ok(0)
            }
            _ => Err(AxError::Unsupported),
        }
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

fn lookup_ion_buffer(file_descriptor: i32) -> AxResult<Arc<IonBuffer>> {
    let file = get_file_like(file_descriptor).map_err(|_| AxError::BadFileDescriptor)?;
    let ion_file: Arc<IonBufferFile> = file
        .downcast_arc::<IonBufferFile>()
        .map_err(|_| AxError::InvalidInput)?;
    Ok(ion_file.buffer().clone())
}

fn resolve_nv12_frame(
    buffer: &IonBuffer,
    y_offset: u64,
    uv_offset: u64,
    size: Size,
    y_stride: u32,
    uv_stride: u32,
) -> AxResult<Nv12Frame> {
    let base = buffer.dma_info.bus_addr.as_u64();
    let y_address = base.checked_add(y_offset).ok_or(AxError::InvalidInput)?;
    let uv_address = base.checked_add(uv_offset).ok_or(AxError::InvalidInput)?;
    let frame = Nv12Frame {
        size,
        y: Plane {
            address: y_address,
            stride: y_stride,
        },
        uv: Plane {
            address: uv_address,
            stride: uv_stride,
        },
    };
    frame.validate().map_err(map_driver_error)?;
    let y_span = frame.y_span().map_err(map_driver_error)?;
    let uv_span = frame.uv_span().map_err(map_driver_error)?;
    validate_buffer_range(buffer.size, y_offset, y_span)?;
    validate_buffer_range(buffer.size, uv_offset, uv_span)?;
    if ranges_overlap(y_offset, y_span, uv_offset, uv_span)? {
        return Err(AxError::InvalidInput);
    }
    Ok(frame)
}

fn resolve_yuv422_planar_frame(
    buffer: &IonBuffer,
    y_offset: u64,
    cb_offset: u64,
    cr_offset: u64,
    size: Size,
    y_stride: u32,
    c_stride: u32,
) -> AxResult<Yuv422PlanarFrame> {
    let base = buffer.dma_info.bus_addr.as_u64();
    let y_address = base.checked_add(y_offset).ok_or(AxError::InvalidInput)?;
    let cb_address = base.checked_add(cb_offset).ok_or(AxError::InvalidInput)?;
    let cr_address = base.checked_add(cr_offset).ok_or(AxError::InvalidInput)?;
    let frame = Yuv422PlanarFrame {
        size,
        y: Plane {
            address: y_address,
            stride: y_stride,
        },
        cb: Plane {
            address: cb_address,
            stride: c_stride,
        },
        cr: Plane {
            address: cr_address,
            stride: c_stride,
        },
    };
    frame.validate().map_err(map_driver_error)?;
    let y_span = frame.y_span().map_err(map_driver_error)?;
    let cb_span = frame.cb_span().map_err(map_driver_error)?;
    let cr_span = frame.cr_span().map_err(map_driver_error)?;
    validate_buffer_range(buffer.size, y_offset, y_span)?;
    validate_buffer_range(buffer.size, cb_offset, cb_span)?;
    validate_buffer_range(buffer.size, cr_offset, cr_span)?;
    if ranges_overlap(y_offset, y_span, cb_offset, cb_span)?
        || ranges_overlap(y_offset, y_span, cr_offset, cr_span)?
        || ranges_overlap(cb_offset, cb_span, cr_offset, cr_span)?
    {
        return Err(AxError::InvalidInput);
    }
    Ok(frame)
}

fn resolve_rgb_planar_frame(
    buffer: &IonBuffer,
    r_offset: u64,
    g_offset: u64,
    b_offset: u64,
    size: Size,
    r_stride: u32,
    gb_stride: u32,
) -> AxResult<RgbPlanarFrame> {
    let base = buffer.dma_info.bus_addr.as_u64();
    let r_address = base.checked_add(r_offset).ok_or(AxError::InvalidInput)?;
    let g_address = base.checked_add(g_offset).ok_or(AxError::InvalidInput)?;
    let b_address = base.checked_add(b_offset).ok_or(AxError::InvalidInput)?;
    let frame = RgbPlanarFrame {
        size,
        r: Plane {
            address: r_address,
            stride: r_stride,
        },
        g: Plane {
            address: g_address,
            stride: gb_stride,
        },
        b: Plane {
            address: b_address,
            stride: gb_stride,
        },
    };
    frame.validate().map_err(map_driver_error)?;
    let r_span = frame.r_span().map_err(map_driver_error)?;
    let g_span = frame.g_span().map_err(map_driver_error)?;
    let b_span = frame.b_span().map_err(map_driver_error)?;
    validate_buffer_range(buffer.size, r_offset, r_span)?;
    validate_buffer_range(buffer.size, g_offset, g_span)?;
    validate_buffer_range(buffer.size, b_offset, b_span)?;
    if ranges_overlap(r_offset, r_span, g_offset, g_span)?
        || ranges_overlap(r_offset, r_span, b_offset, b_span)?
        || ranges_overlap(g_offset, g_span, b_offset, b_span)?
    {
        return Err(AxError::InvalidInput);
    }
    Ok(frame)
}

fn ranges_overlap(
    first_offset: u64,
    first_span: u64,
    second_offset: u64,
    second_span: u64,
) -> AxResult<bool> {
    let first_end = first_offset
        .checked_add(first_span)
        .ok_or(AxError::InvalidInput)?;
    let second_end = second_offset
        .checked_add(second_span)
        .ok_or(AxError::InvalidInput)?;
    Ok(first_offset < second_end && second_offset < first_end)
}

fn apply_execution_to_nv12_request(
    request: &mut VpssRun,
    executed: &ExecutedJob,
    timestamp_ns: u64,
) {
    request.hardware_start_ns = executed.hardware_start_ns;
    request.hardware_done_ns = executed.hardware_done_ns;
    request.elapsed_ns = executed.elapsed_ns;
    request.irq_status = executed.completion.irq_status;
    request.img_debug = executed.completion.diagnostics.img_debug;
    request.img_axi_status = executed.completion.diagnostics.img_axi_status;
    request.scaler_status = executed.completion.diagnostics.scaler_status;
    request.odma_debug = executed.completion.diagnostics.odma_debug;
    request.output_sequence = executed.completion.sequence;
    request.output_timestamp_ns = timestamp_ns;
}

fn apply_execution_to_yuv422p_request(
    request: &mut VpssRunYuv422p,
    executed: &ExecutedJob,
    timestamp_ns: u64,
) {
    request.hardware_start_ns = executed.hardware_start_ns;
    request.hardware_done_ns = executed.hardware_done_ns;
    request.elapsed_ns = executed.elapsed_ns;
    request.irq_status = executed.completion.irq_status;
    request.img_debug = executed.completion.diagnostics.img_debug;
    request.img_axi_status = executed.completion.diagnostics.img_axi_status;
    request.scaler_status = executed.completion.diagnostics.scaler_status;
    request.odma_debug = executed.completion.diagnostics.odma_debug;
    request.output_sequence = executed.completion.sequence;
    request.output_timestamp_ns = timestamp_ns;
}

fn apply_execution_to_yuv422p_rgb_request(
    request: &mut VpssRunYuv422pRgb,
    executed: &ExecutedJob,
    timestamp_ns: u64,
) {
    request.hardware_start_ns = executed.hardware_start_ns;
    request.hardware_done_ns = executed.hardware_done_ns;
    request.elapsed_ns = executed.elapsed_ns;
    request.irq_status = executed.completion.irq_status;
    request.img_debug = executed.completion.diagnostics.img_debug;
    request.img_axi_status = executed.completion.diagnostics.img_axi_status;
    request.scaler_status = executed.completion.diagnostics.scaler_status;
    request.odma_debug = executed.completion.diagnostics.odma_debug;
    request.output_sequence = executed.completion.sequence;
    request.output_timestamp_ns = timestamp_ns;
}

fn validate_buffer_range(buffer_size: usize, offset: u64, span: u64) -> AxResult<()> {
    let end = offset.checked_add(span).ok_or(AxError::InvalidInput)?;
    if end > buffer_size as u64 {
        return Err(AxError::InvalidInput);
    }
    Ok(())
}

fn enable_vpss_clocks() -> AxResult<()> {
    let mapping = axklib::mem::iomap(PhysAddr::from(CLKGEN_BASE), CLKGEN_MMIO_SIZE)?;
    let base = mapping.as_mut_ptr();
    // SAFETY: CLKGEN is a mapped device page and both offsets are aligned
    // 32-bit gate registers. Preserve firmware-selected parents/dividers and
    // every unrelated gate bit.
    unsafe {
        let enable_2 = base.add(CLK_ENABLE_2).cast::<u32>();
        let enable_3 = base.add(CLK_ENABLE_3).cast::<u32>();
        fence(Ordering::SeqCst);
        enable_2.write_volatile(enable_2.read_volatile() | CLK_ENABLE_2_MASK);
        enable_3.write_volatile(enable_3.read_volatile() | CLK_ENABLE_3_MASK);
        fence(Ordering::SeqCst);
    }
    Ok(())
}

fn map_driver_error(error: VpssError) -> AxError {
    match error {
        VpssError::Busy => AxError::ResourceBusy,
        VpssError::Timeout => AxError::TimedOut,
        VpssError::InvalidDimension
        | VpssError::InvalidCrop
        | VpssError::InvalidOutputRect
        | VpssError::InvalidStride
        | VpssError::InvalidAddress => AxError::InvalidInput,
        VpssError::NotInitialized | VpssError::ProgramLate | VpssError::BadState => AxError::Io,
    }
}

fn status_from_error(error: AxError) -> i32 {
    match error {
        AxError::InvalidInput | AxError::BadFileDescriptor => VPSS_STATUS_INVALID,
        AxError::ResourceBusy => VPSS_STATUS_BUSY,
        AxError::TimedOut => VPSS_STATUS_TIMEOUT,
        _ => VPSS_STATUS_IO,
    }
}

fn now_ns() -> u64 {
    ax_runtime::hal::time::monotonic_time_nanos() as u64
}
