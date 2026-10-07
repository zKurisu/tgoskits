//! StarryOS integration for the CV181x VPSS offline scaler path.

use alloc::sync::Arc;
use core::{
    ptr::NonNull,
    sync::atomic::{AtomicU64, Ordering, fence},
    time::Duration,
};

// 分支原版跑在旧内核上，用的是 ax_errno / ax_sync / ax_task / starry_vm；
// 现代内核里这些分别对应 crate::StarryError、crate::sync、ax_std 的任务 re-export
// 与 crate::mm。StarryError 的变体名与 AxError 基本一一对应，所以用别名即可。
use crate::{StarryError as AxError, StarryResult as AxResult};
use ax_memory_addr::PhysAddr;
use axfs_ng_vfs::VfsResult;
use ax_std::os::arceos::{
    task as scheduler,
    task::sync::{
        WaitQueue,
        irq::{IrqRegisterResult, IrqWaitCell, IrqWaitRegistration},
    },
};
use sg200x_bsp::soc::CLKGEN_BASE;
use sg2002_tpu::ion::IonBuffer;
use sg2002_vpss::{
    CompletionState, DestinationFrame, Error as VpssError, Job, JobCompletion, MmioRegion,
    Nv12Frame, Plane, Rect, RgbColor, RgbPlanarFrame, Size, SourceFrame, VpssControl,
    Yuv422PlanarFrame,
    types::{MAX_DIMENSION, MIN_DIMENSION, STRIDE_ALIGNMENT},
};
use crate::mm::VmPtr;
use crate::sync::Mutex;

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
/// SC_V1 normally completes a 640x480 -> 384x384 job in about 1.5 ms.  A
/// sleeping waiter can otherwise miss the immediate IRQ reschedule and wait
/// for the remainder of the 50 ms RR time slice.  Poll only the IRQ-owned
/// atomic completion state for a short, bounded interval, then fall back to
/// the wait queue for slow or abnormal jobs.  The task side must not read or
/// clear the W1C interrupt status register.
const VPSS_COMPLETION_SPIN_NS: u64 = 3_000_000;
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

/// IRQ→任务唤醒通道。IRQ 侧只允许 `notify()`；本树的 `WaitQueue::notify_all()`
/// 带 `assert_task_context_notification()`，在中断上下文会直接 panic。
static VPSS_IRQ_NOTIFY: IrqWaitCell = IrqWaitCell::new();
/// 等待侧真正 park 的队列。
static VPSS_IRQ_PARK: WaitQueue = WaitQueue::new();

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
        let irq_completion = Arc::clone(&completion);
        let irq = resource.irq;
        let registration = request_shared_disabled(irq, move |_| match handler.handle() {
            Some(event) => {
                if event.wake_waiter {
                    // Capture completion in IRQ context before waking the
                    // waiter. Task wake-up latency must not be charged to
                    // VPSS hardware execution.
                    irq_completion.record_finished_at_ns(now_ns());
                    let _ = VPSS_IRQ_NOTIFY.notify();
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
            "[VPSS] IRQ {irq:?} registered and enabled (mmio={:#x}+{:#x})",
            resource.mmio_physical, resource.mmio_size
        );

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

    fn run_ioctl(
        &self,
        current: &crate::task::UserTaskRef,
        user_address: usize,
    ) -> AxResult<usize> {
        let user_pointer = user_address as *mut VpssRun;
        let mut request = user_pointer.vm_read(current)?;
        request.clear_output();
        request.queue_enter_ns = now_ns();
        let result = self.run(&mut request);
        request.status = match &result {
            Ok(()) => VPSS_STATUS_OK,
            Err(error) => status_from_error(error),
        };
        write_user_record(current, user_pointer, &request)?;
        result.map(|()| 0)
    }

    fn run_yuv422p_ioctl(
        &self,
        current: &crate::task::UserTaskRef,
        user_address: usize,
    ) -> AxResult<usize> {
        let user_pointer = user_address as *mut VpssRunYuv422p;
        let mut request = user_pointer.vm_read(current)?;
        request.clear_output();
        request.queue_enter_ns = now_ns();
        let result = self.run_yuv422p(&mut request);
        request.status = match &result {
            Ok(()) => VPSS_STATUS_OK,
            Err(error) => status_from_error(error),
        };
        write_user_record(current, user_pointer, &request)?;
        result.map(|()| 0)
    }

    fn run_yuv422p_rgb_ioctl(
        &self,
        current: &crate::task::UserTaskRef,
        user_address: usize,
    ) -> AxResult<usize> {
        let user_pointer = user_address as *mut VpssRunYuv422pRgb;
        let mut request = user_pointer.vm_read(current)?;
        request.clear_output();
        request.queue_enter_ns = now_ns();
        let result = self.run_yuv422p_rgb(&mut request);
        request.status = match &result {
            Ok(()) => VPSS_STATUS_OK,
            Err(error) => status_from_error(error),
        };
        write_user_record(current, user_pointer, &request)?;
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
        let mut hardware_start_ns = 0;
        control
            .start_with_hook(job, || hardware_start_ns = now_ns())
            .map_err(map_driver_error)?;
        let timeout_ms = match timeout_ms {
            0 => VPSS_DEFAULT_TIMEOUT_MS,
            value => value.min(VPSS_MAX_TIMEOUT_MS),
        };
        let absolute_deadline_ns = hardware_start_ns
            .saturating_add(u64::from(timeout_ms).saturating_mul(1_000_000));
        let spin_deadline_ns = hardware_start_ns
            .saturating_add(VPSS_COMPLETION_SPIN_NS)
            .min(absolute_deadline_ns);
        while !self.completion.is_finished() && now_ns() < spin_deadline_ns {
            core::hint::spin_loop();
        }
        let timed_out = if self.completion.is_finished() {
            false
        } else {
            let remaining_ns = absolute_deadline_ns.saturating_sub(now_ns());
            if remaining_ns == 0 {
                true
            } else {
                !wait_for_completion(remaining_ns, &self.completion)
            }
        };
        if timed_out && !self.completion.is_finished() {
            // 关键诊断：区分"硬件已置位但中断没送到 CPU"与"VPSS 根本没完成"。
            warn!(
                "[VPSS] completion wait timed out after {} ms: TOP_INTR_STATUS=0x{:x} stats={:?}",
                timeout_ms,
                control.interrupt_status(),
                self.completion.stats()
            );
        }
        let completion = if timed_out && !self.completion.is_finished() {
            control.recover_timeout().map_err(map_driver_error)?
        } else {
            control.finish().map_err(map_driver_error)?
        };
        // The IRQ handler records the terminal event before waking us. Keep a
        // fallback for timeout/recovery paths where no terminal IRQ exists.
        let irq_done_ns = self.completion.finished_at_ns();
        let hardware_done_ns = if irq_done_ns != 0 {
            irq_done_ns
        } else {
            now_ns()
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
    fn ioctl_inner(
        &self,
        current: &crate::task::UserTaskRef,
        command: u32,
        argument: usize,
    ) -> AxResult<usize> {
        match command {
            VPSS_IOCTL_RUN => self.run_ioctl(current, argument),
            VPSS_IOCTL_RUN_YUV422P => self.run_yuv422p_ioctl(current, argument),
            VPSS_IOCTL_RUN_YUV422P_RGB => self.run_yuv422p_rgb_ioctl(current, argument),
            VPSS_IOCTL_GET_INFO => {
                write_user_record(current, argument as *mut VpssInfo, &self.info())?;
                Ok(0)
            }
            VPSS_IOCTL_GET_STATS => {
                write_user_record(current, argument as *mut VpssStats, &self.stats())?;
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
}

impl DeviceOps for VpssDevice {
    fn read_at(&self, _buffer: &mut [u8], _offset: u64) -> VfsResult<usize> {
        Ok(0)
    }

    fn write_at(&self, _buffer: &[u8], _offset: u64) -> VfsResult<usize> {
        Ok(0)
    }

    fn ioctl(
        &self,
        current: &crate::task::UserTaskRef,
        command: u32,
        argument: usize,
    ) -> VfsResult<usize> {
        self.ioctl_inner(current, command, argument)
            .map_err(Into::into)
    }


    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}

/// 把一份 `repr(C)` 的 ABI 记录整块写进用户内存。
///
/// 这些结构体带 padding，不满足 `bytemuck::NoUninit`（给它 derive 会直接编译
/// 报错 "applied to a type with padding"），所以不能走 `vm_write`；按字节拷贝
/// 即可（`u8: NoUninit`）。
fn write_user_record<T>(
    current: &crate::task::UserTaskRef,
    pointer: *mut T,
    value: &T,
) -> AxResult<()> {
    // SAFETY: `value` 是已初始化的 repr(C) 记录，按 `size_of::<T>()` 取它的字节
    // 表示是合法的；这里只读，不写。
    let bytes = unsafe {
        core::slice::from_raw_parts(value as *const T as *const u8, core::mem::size_of::<T>())
    };
    crate::mm::vm_write_slice::<u8>(current, pointer as *mut u8, bytes)?;
    Ok(())
}

/// 等待一次 VPSS 完成（IRQ 侧已经 `VPSS_IRQ_NOTIFY.notify()`）。
///
/// 分支原版直接调 `DONE_WAIT_QUEUE.notify_all_from_irq()`；本树没有这个方法，
/// 而 `WaitQueue::notify_all()` 要求任务上下文，所以按 `dev/tpu` 的写法承载：
/// `IrqWaitCell` 的 pending/register 握手覆盖"IRQ 早于注册"，park 队列的
/// generation 再覆盖"唤醒早于 park"。返回 `true` 表示观察到完成。
fn wait_for_completion(timeout_ns: u64, completion: &CompletionState) -> bool {
    let current = scheduler::thread::current::current_thread_handle()
        .unwrap_or_else(|error| panic!("VPSS waiter has no scheduler thread: {error}"));
    let registration = IrqWaitRegistration::new(current.wake_handle());
    match VPSS_IRQ_NOTIFY.register(&registration) {
        IrqRegisterResult::ConsumedPending => completion.is_finished(),
        IrqRegisterResult::Registered(token) | IrqRegisterResult::NotificationInFlight(token) => {
            let _timed_out = VPSS_IRQ_PARK.wait_timeout_until(
                Duration::from_nanos(timeout_ns),
                || !token.is_attached() || completion.is_finished(),
            );
            scheduler::sync::irq::quiesce_irq_wait(token)
                .unwrap_or_else(|error| panic!("VPSS IRQ waiter could not quiesce: {error}"));
            completion.is_finished()
        }
        IrqRegisterResult::Occupied => completion.is_finished(),
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
    let base = buffer.dma_addr().as_u64();
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
    let base = buffer.dma_addr().as_u64();
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
    let base = buffer.dma_addr().as_u64();
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
    // `axklib::mem::iomap` 返回的是 KlibError，本树的 StarryError 没有它的
    // From 实现，所以在边界显式折算成 Io。
    let mapping = axklib::mem::iomap(PhysAddr::from(CLKGEN_BASE), CLKGEN_MMIO_SIZE).map_err(
        |error| {
            warn!("[VPSS] failed to map CLKGEN: {error:?}");
            AxError::Io
        },
    )?;
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

fn status_from_error(error: &AxError) -> i32 {
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
