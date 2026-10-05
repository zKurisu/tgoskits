//! TPU 设备 OS 适配
//!
//! 将 ioctl 命令翻译为 `Sg2002Tpu` 调用，并通过 fd 解析 Ion buffer
//! 物理/虚拟地址。
//!
//! 默认使用同步直通模型：`submit` 在调用线程中串行调用
//! [`Sg2002Tpu::run_one`]，等待 TDMA 完成时通过 `IRQ_WQ` 睡眠让出 CPU，
//! 完成结果仍放入 `DONE_LIST`，随后 `wait` 按 `(tid, seq_no)` 取回。这保持
//! cviruntime 的 submit/wait ABI，同时消除 submit→worker→waiter 两次调度交接。
//! 文件中保留异步 worker 路径，供 `TPU_DIRECT_EXECUTION` A/B 回退。
//!
//! SG2002 默认单核，执行任务的线程等待硬件时必须真正睡眠让出 CPU，相机
//! 前处理才能与 TPU 推理重叠。
//!
//! # 接口约定（重要）
//!
//! - **`submit` 与 `wait` 必须在同一线程调用。** 完成项以 `(提交线程 tid,
//!   用户 seq_no)` 为匹配键存入全局 `DONE_LIST`；`wait` 用「当前线程 tid +
//!   传入 seq_no」检索。换线程 `wait` 会查不到结果而超时。该约束等价于原
//!   Linux 驱动以 `current->pid` 隔离任务的语义，并隔离了不同进程/线程偶然
//!   使用相同 `seq_no` 时的串扰（否则一个 waiter 可能取走他人的完成项）。
//! - **`seq_no` 由用户态提供，仅需在「同一线程的在途请求之间」唯一。** 它不是
//!   内核分配的全局令牌；跨线程不保证唯一也无需唯一，因为 tid 已隔离。
//! - **buffer 生命周期：** `submit` 入队的 [`TpuTask`] 持有底层 Ion buffer 的
//!   `Arc` 强引用，直到结果被 `wait` 取走（或因 `DONE_LIST` 超限被丢弃）。
//!   因此用户在 worker 跑完前 `close(fd)` 不会导致 DMA 物理页被回收
//!   （防 use-after-free）。

use alloc::{collections::VecDeque, string::String, sync::Arc};
use core::{
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering},
    time::Duration,
};

use ax_kspin::SpinNoIrq;
use ax_memory_addr::PhysAddr;
use ax_runtime::hal::time::monotonic_time_nanos;
use ax_sync::Mutex;
use ax_task::WaitQueue;
use sg2002_tpu::{
    ion::IonBuffer,
    tpu::{
        Sg2002Tpu,
        error::TpuError,
        types::{
            CVITPU_DMABUF_FLUSH, CVITPU_DMABUF_FLUSH_FD, CVITPU_DMABUF_INVLD,
            CVITPU_DMABUF_INVLD_FD, CVITPU_LOAD_TEE, CVITPU_PIO_MODE, CVITPU_SUBMIT_DMABUF,
            CVITPU_SUBMIT_TEE, CVITPU_UNLOAD_TEE, CVITPU_WAIT_DMABUF, CviCacheOpArg,
            CviSubmitDmaArg, CviWaitDmaArg, DmaHeader,
        },
    },
};
use starry_vm::{VmMutPtr, VmPtr};

use crate::{
    file::{get_file_like, ion::IonBufferFile},
    pseudofs::{
        DeviceOps,
        dev::{IrqRegistration, request_shared_disabled},
    },
};

/// 一个 TPU 推理任务（OS glue 侧）。
struct TpuTask {
    /// 提交线程 id。与 `seq_no` 组成复合匹配键，隔离跨进程/线程的相同 seq_no
    /// （对应原 Linux 驱动 `node->pid = current->pid` 的隔离语义）。
    tid: u64,
    /// 序列号，submit / wait 通过 `(tid, seq_no)` 配对结果。
    seq_no: u32,
    /// DMA buffer 虚拟地址。
    vaddr: usize,
    /// DMA buffer 物理地址。
    paddr: u64,
    /// 持有底层 Ion buffer 的强引用，保证 worker 跑硬件、结果被取走之前，
    /// 即使用户提前 close fd，物理 DMA 页也不会被回收（防 use-after-free）。
    _buffer: Arc<IonBuffer>,
    /// 执行结果（0 成功，-1 失败），由 worker 回填。
    ret: i32,
    /// Kernel-side latency timestamps and accumulated IRQ segments.
    submit_ns: u64,
    worker_start_ns: u64,
    worker_done_ns: u64,
    fire_to_irq_ns: u64,
    irq_to_worker_resume_ns: u64,
    tdma_irq_count: u64,
    /// Command-buffer class used to keep tensor import separate from model
    /// execution in latency reports.
    kind: TpuTaskKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TpuTaskKind {
    /// A command buffer containing GDMA only.  cviruntime uses this path to
    /// import the prepared input tensor into TPU memory.
    TdmaOnly = 0,
    /// A command buffer containing at least one TIU/BD descriptor.  This is
    /// the actual network forward path (and may also contain GDMA commands).
    Compute  = 1,
    /// Malformed or otherwise unrecognised command-buffer header.
    Unknown  = 2,
}

impl TpuTaskKind {
    const COUNT: usize = 3;

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Default)]
struct TpuTimingAggregate {
    samples: u64,
    submit_to_worker_ns: u64,
    fire_to_irq_ns: u64,
    irq_to_worker_resume_ns: u64,
    worker_done_to_user_ns: u64,
    kernel_total_ns: u64,
    other_kernel_ns: u64,
    tdma_irq_count: u64,
}

#[derive(Clone, Copy)]
struct TpuTimingSample {
    submit_to_run_ns: u64,
    fire_to_irq_ns: u64,
    irq_to_run_resume_ns: u64,
    run_done_to_user_ns: u64,
    other_kernel_ns: u64,
    kernel_total_ns: u64,
    tdma_irq_count: u64,
}

impl TpuTimingAggregate {
    const ZERO: Self = Self {
        samples: 0,
        submit_to_worker_ns: 0,
        fire_to_irq_ns: 0,
        irq_to_worker_resume_ns: 0,
        worker_done_to_user_ns: 0,
        kernel_total_ns: 0,
        other_kernel_ns: 0,
        tdma_irq_count: 0,
    };

    fn add(&mut self, sample: TpuTimingSample) {
        self.samples = self.samples.saturating_add(1);
        self.submit_to_worker_ns = self
            .submit_to_worker_ns
            .saturating_add(sample.submit_to_run_ns);
        self.fire_to_irq_ns = self.fire_to_irq_ns.saturating_add(sample.fire_to_irq_ns);
        self.irq_to_worker_resume_ns = self
            .irq_to_worker_resume_ns
            .saturating_add(sample.irq_to_run_resume_ns);
        self.worker_done_to_user_ns = self
            .worker_done_to_user_ns
            .saturating_add(sample.run_done_to_user_ns);
        self.other_kernel_ns = self.other_kernel_ns.saturating_add(sample.other_kernel_ns);
        self.kernel_total_ns = self.kernel_total_ns.saturating_add(sample.kernel_total_ns);
        self.tdma_irq_count = self.tdma_irq_count.saturating_add(sample.tdma_irq_count);
    }
}

/// 待执行任务队列（对应 Linux `task_list`）。
static TASK_LIST: SpinNoIrq<VecDeque<TpuTask>> = SpinNoIrq::new(VecDeque::new());
/// 已完成任务队列（对应 Linux `done_list`）。
static DONE_LIST: SpinNoIrq<VecDeque<TpuTask>> = SpinNoIrq::new(VecDeque::new());
/// `DONE_LIST` 上限。每个滞留完成项持有一个 `Arc<IonBuffer>`，提交后不 wait
/// 的线程会令其无限累积；超限丢弃最旧项以释放 buffer（对应原驱动
/// `DONE_LIST_MAX`）。
const DONE_LIST_MAX: usize = 64;
/// 唤醒 worker 取任务（对应 Linux `task_wait_queue`）。
static TASK_WQ: WaitQueue = WaitQueue::new();
/// 唤醒等待结果的提交者（对应 Linux `done_wait_queue`）。
static DONE_WQ: WaitQueue = WaitQueue::new();
/// TDMA 硬件中断到达时唤醒在此睡眠的 worker。
static IRQ_WQ: WaitQueue = WaitQueue::new();
/// Serialises direct callers while allowing the owner to sleep on TDMA IRQs.
/// Unlike a spin lock, this mutex is deliberately held across `run_one`.
static TPU_RUN_LOCK: Mutex<()> = Mutex::new(());
/// worker 线程是否已启动（保证只 spawn 一次）。
static WORKER_SPAWNED: AtomicBool = AtomicBool::new(false);
/// 指向唯一 TPU 硬件实例，供注入的 [`tpu_wait_irq`] 读取中断标志。
///
/// SG2002 只有一个 TPU；`Sg2002Tpu` 由 worker 持有的 `Arc` 保活，实际生命
/// 周期与内核同长，这里的裸指针始终有效。
static HW_PTR: AtomicPtr<Sg2002Tpu> = AtomicPtr::new(core::ptr::null_mut());

/// Per-request TDMA timing scratch.  There is exactly one TPU worker, so only
/// one hardware submission can update these counters at a time.
static LAST_TDMA_FIRE_NS: AtomicU64 = AtomicU64::new(0);
static LAST_TDMA_IRQ_NS: AtomicU64 = AtomicU64::new(0);
static ACTIVE_FIRE_TO_IRQ_NS: AtomicU64 = AtomicU64::new(0);
static ACTIVE_IRQ_TO_RESUME_NS: AtomicU64 = AtomicU64::new(0);
static ACTIVE_TDMA_IRQ_COUNT: AtomicU64 = AtomicU64::new(0);
/// The run lock permits only one active command buffer. Its GDMA-only
/// completion can use a one-shot front-of-queue wakeup.
static ACTIVE_TDMA_ONLY: AtomicBool = AtomicBool::new(false);
static TIMING_AGGREGATE: SpinNoIrq<TpuTimingAggregate> = SpinNoIrq::new(TpuTimingAggregate::ZERO);
static KIND_TIMING_AGGREGATES: SpinNoIrq<[TpuTimingAggregate; TpuTaskKind::COUNT]> =
    SpinNoIrq::new([TpuTimingAggregate::ZERO; TpuTaskKind::COUNT]);
const TPU_TIMING_REPORT_INTERVAL: u64 = 100;
/// A/B switch: execute in the submitting task instead of bouncing through the
/// worker and DONE wait queues.  The submit/wait ioctl ABI and DONE_LIST result
/// matching remain unchanged; only submit becomes completion-synchronous.
const TPU_DIRECT_EXECUTION: bool = true;

/// TPU 字符设备
pub struct TpuDevice {
    /// 硬件层
    hw: Arc<Sg2002Tpu>,
    resource: TpuResource,
    /// TDMA IRQ action registration.
    irq_registration: Option<IrqRegistration>,
}

const TPU_COMPATIBLES: &[&str] = &["cvitek,tpu"];
const TPU_TDMA_IRQ_NAME: &str = "tdma_irq";
const TPU_DEFAULT_MMIO_SIZE: usize = 0x1000;

/// 等待 TDMA 完成的总超时（约 10 秒）。
const TPU_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
struct TpuResource {
    tdma_paddr: usize,
    tdma_size: usize,
    tiu_paddr: usize,
    tiu_size: usize,
    irq: Option<ax_runtime::hal::irq::IrqId>,
}

#[inline]
fn timing_now_ns() -> u64 {
    monotonic_time_nanos() as u64
}

fn reset_active_tdma_timing() {
    LAST_TDMA_FIRE_NS.store(0, Ordering::Release);
    LAST_TDMA_IRQ_NS.store(0, Ordering::Release);
    ACTIVE_FIRE_TO_IRQ_NS.store(0, Ordering::Release);
    ACTIVE_IRQ_TO_RESUME_NS.store(0, Ordering::Release);
    ACTIVE_TDMA_IRQ_COUNT.store(0, Ordering::Release);
}

/// Called by the hardware core immediately before each TDMA descriptor or PMU
/// completion is fired.
fn mark_tdma_fire() {
    LAST_TDMA_IRQ_NS.store(0, Ordering::Release);
    LAST_TDMA_FIRE_NS.store(timing_now_ns(), Ordering::Release);
}

/// Called in worker context immediately after the IRQ wait returns.
fn mark_tdma_worker_resume() {
    let now = timing_now_ns();
    let irq = LAST_TDMA_IRQ_NS.swap(0, Ordering::AcqRel);
    if irq != 0 {
        ACTIVE_IRQ_TO_RESUME_NS.fetch_add(now.saturating_sub(irq), Ordering::AcqRel);
    }
}

fn classify_tpu_task(buffer: &IonBuffer) -> TpuTaskKind {
    if buffer.size < core::mem::size_of::<DmaHeader>() {
        return TpuTaskKind::Unknown;
    }
    let header = unsafe { &*(buffer.dma_info.cpu_addr.as_ptr() as *const DmaHeader) };
    if !header.is_valid() {
        TpuTaskKind::Unknown
    } else if header.bd_desc_count == 0 && header.tdma_desc_count > 0 {
        TpuTaskKind::TdmaOnly
    } else if header.bd_desc_count > 0 {
        TpuTaskKind::Compute
    } else {
        TpuTaskKind::Unknown
    }
}

fn log_timing_aggregate(prefix: &str, aggregate: TpuTimingAggregate) {
    if aggregate.samples == 0 {
        return;
    }
    let samples = aggregate.samples;
    info!(
        "[TPU] {} samples={} submit_to_run_avg_us={} fire_to_irq_avg_us={} \
         irq_to_run_resume_avg_us={} run_done_to_user_avg_us={} other_kernel_avg_us={} \
         kernel_total_avg_us={} irq_per_task_x100={} path={}",
        prefix,
        samples,
        aggregate.submit_to_worker_ns / samples / 1_000,
        aggregate.fire_to_irq_ns / samples / 1_000,
        aggregate.irq_to_worker_resume_ns / samples / 1_000,
        aggregate.worker_done_to_user_ns / samples / 1_000,
        aggregate.other_kernel_ns / samples / 1_000,
        aggregate.kernel_total_ns / samples / 1_000,
        aggregate.tdma_irq_count.saturating_mul(100) / samples,
        if TPU_DIRECT_EXECUTION {
            "direct"
        } else {
            "worker"
        },
    );
}

fn record_tpu_timing(task: &TpuTask, user_return_ns: u64) {
    let submit_to_worker = task.worker_start_ns.saturating_sub(task.submit_ns);
    let worker_done_to_user = user_return_ns.saturating_sub(task.worker_done_ns);
    let kernel_total = user_return_ns.saturating_sub(task.submit_ns);
    let accounted = submit_to_worker
        .saturating_add(task.fire_to_irq_ns)
        .saturating_add(task.irq_to_worker_resume_ns)
        .saturating_add(worker_done_to_user);
    let other_kernel = kernel_total.saturating_sub(accounted);

    let sample = TpuTimingSample {
        submit_to_run_ns: submit_to_worker,
        fire_to_irq_ns: task.fire_to_irq_ns,
        irq_to_run_resume_ns: task.irq_to_worker_resume_ns,
        run_done_to_user_ns: worker_done_to_user,
        other_kernel_ns: other_kernel,
        kernel_total_ns: kernel_total,
        tdma_irq_count: task.tdma_irq_count,
    };

    let report_total = {
        let mut aggregate = TIMING_AGGREGATE.lock();
        aggregate.add(sample);

        if aggregate.samples.is_multiple_of(TPU_TIMING_REPORT_INTERVAL) {
            Some(*aggregate)
        } else {
            None
        }
    };

    let report_kinds = {
        let mut aggregates = KIND_TIMING_AGGREGATES.lock();
        aggregates[task.kind.index()].add(sample);
        report_total.map(|_| *aggregates)
    };

    if let (Some(total), Some(kinds)) = (report_total, report_kinds) {
        log_timing_aggregate("TASK_TIMING kind=all", total);
        for kind in [
            TpuTaskKind::TdmaOnly,
            TpuTaskKind::Compute,
            TpuTaskKind::Unknown,
        ] {
            log_timing_aggregate(
                match kind {
                    TpuTaskKind::TdmaOnly => "TASK_TIMING kind=tdma_only",
                    TpuTaskKind::Compute => "TASK_TIMING kind=compute",
                    TpuTaskKind::Unknown => "TASK_TIMING kind=unknown",
                },
                kinds[kind.index()],
            );
        }
    }
}

impl TpuResource {
    fn probe() -> Option<Self> {
        let resource = Self::from_fdt();
        if resource.is_none() {
            warn!("[TPU] cvitek,tpu node not found or invalid in FDT");
        }
        resource
    }

    fn from_fdt() -> Option<Self> {
        rdrive::with_fdt(|fdt| {
            fdt.find_compatible(TPU_COMPATIBLES)
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

        let mut regs = node.regs().into_iter();
        let tdma = regs.next()?;
        let tiu = regs.next()?;
        let irq = match resolve_named_fdt_irq(&node, TPU_TDMA_IRQ_NAME) {
            Ok(irq) => irq,
            Err(err) => {
                warn!("[TPU] failed to resolve {TPU_TDMA_IRQ_NAME}: {err:?}");
                return None;
            }
        };

        Some(Self {
            tdma_paddr: tdma.address as usize,
            tdma_size: tdma.size.unwrap_or(TPU_DEFAULT_MMIO_SIZE as u64) as usize,
            tiu_paddr: tiu.address as usize,
            tiu_size: tiu.size.unwrap_or(TPU_DEFAULT_MMIO_SIZE as u64) as usize,
            irq,
        })
    }
}

fn resolve_named_fdt_irq(
    node: &rdrive::probe::fdt::NodeType<'_>,
    name: &str,
) -> Result<Option<ax_runtime::hal::irq::IrqId>, ax_runtime::hal::irq::IrqError> {
    let Some(irq) = ax_driver::binding_irq_from_named_fdt_interrupt(node, name)
        .map_err(|_| ax_runtime::hal::irq::IrqError::Unsupported)?
    else {
        return Ok(None);
    };
    ax_runtime::irq::resolve_binding_irq(irq).map(Some)
}

fn map_tpu_mmio(resource: TpuResource) -> Option<(*mut u8, *mut u8)> {
    let tdma = match axklib::mem::iomap(PhysAddr::from(resource.tdma_paddr), resource.tdma_size) {
        Ok(vaddr) => vaddr.as_mut_ptr(),
        Err(err) => {
            warn!(
                "[TPU] failed to map TDMA MMIO at {:#x}+{:#x}: {err:?}",
                resource.tdma_paddr, resource.tdma_size
            );
            return None;
        }
    };
    let tiu = match axklib::mem::iomap(PhysAddr::from(resource.tiu_paddr), resource.tiu_size) {
        Ok(vaddr) => vaddr.as_mut_ptr(),
        Err(err) => {
            warn!(
                "[TPU] failed to map TIU MMIO at {:#x}+{:#x}: {err:?}",
                resource.tiu_paddr, resource.tiu_size
            );
            return None;
        }
    };
    Some((tdma, tiu))
}

fn register_tpu_irq(
    irq: Option<ax_runtime::hal::irq::IrqId>,
    hw: &Arc<Sg2002Tpu>,
) -> Option<IrqRegistration> {
    let Some(irq) = irq else {
        warn!("[TPU] TDMA IRQ not available; execution will use MMIO poll fallback");
        return None;
    };
    let hw = Arc::clone(hw);
    let registration = match request_shared_disabled(irq, move |_| {
        let irq_ns = timing_now_ns();
        if hw.handle_irq() {
            warn!("[TPU] TDMA IRQ {irq:?} reports error status");
        }
        // `LAST_TDMA_FIRE_NS` is consumed exactly once for each real TDMA
        // completion. Spurious/shared IRQs see zero and are not profiled.
        let fire_ns = LAST_TDMA_FIRE_NS.swap(0, Ordering::AcqRel);
        if fire_ns != 0 {
            ACTIVE_FIRE_TO_IRQ_NS.fetch_add(irq_ns.saturating_sub(fire_ns), Ordering::AcqRel);
            ACTIVE_TDMA_IRQ_COUNT.fetch_add(1, Ordering::AcqRel);
            LAST_TDMA_IRQ_NS.store(irq_ns, Ordering::Release);
        }
        // TDMA normally finishes far inside the 50 ms RR time slice.  Merely
        // unblocking the worker without requesting an IRQ-exit reschedule can
        // therefore add almost a full time slice to every TPU submission.
        // Use the IRQ-safe wake helper so the worker can finish the request as
        // soon as the interrupt returns.
        if fire_ns != 0 && ACTIVE_TDMA_ONLY.load(Ordering::Acquire) {
            IRQ_WQ.notify_one_front_force_from_irq();
        } else {
            IRQ_WQ.notify_all_force_from_irq();
        }
        ax_runtime::hal::irq::IrqReturn::Handled
    }) {
        Ok(registration) => registration,
        Err(err) => {
            warn!("[TPU] failed to register TDMA IRQ {irq:?}: {err:?}");
            return None;
        }
    };
    if let Err(err) = registration.enable() {
        warn!("[TPU] failed to enable TDMA IRQ {irq:?}: {err:?}");
        return None;
    }
    info!("[TPU] TDMA IRQ {irq:?} registered and enabled");
    Some(registration)
}

/// 注入给 driver core 的阻塞等待函数：在超时内睡眠等待 TDMA 中断到达。
///
/// 由 worker 线程上下文调用（普通可调度任务），睡眠让出 CPU；硬件中断到达时
/// `tpu_tdma_irq_handler` 经 `IRQ_WQ` 唤醒。返回 `true` 表示中断已到达，
/// `false` 表示本轮超时。
fn tpu_wait_irq(timeout_us: u64) -> bool {
    let hw = HW_PTR.load(Ordering::Acquire);
    if hw.is_null() {
        return false;
    }
    // SAFETY: HW_PTR 指向 worker 持有的 Arc 内的实例，生命周期与内核同长。
    let hw = unsafe { &*hw };
    // wait_timeout_until 在睡前于队列锁内复检谓词，等价 Linux wait_event，
    // 无唤醒先于等待的丢失风险。返回 true 表示超时。
    !IRQ_WQ.wait_timeout_until(Duration::from_micros(timeout_us), || hw.irq_pending())
}

fn run_tpu_task(hw: &Sg2002Tpu, task: &mut TpuTask) {
    task.worker_start_ns = timing_now_ns();
    reset_active_tdma_timing();
    ACTIVE_TDMA_ONLY.store(task.kind == TpuTaskKind::TdmaOnly, Ordering::Release);
    task.ret = hw
        .run_one(task.seq_no, task.vaddr, task.paddr)
        .map_or_else(|error| error.as_errno(), |_| 0);
    ACTIVE_TDMA_ONLY.store(false, Ordering::Release);
    task.worker_done_ns = timing_now_ns();
    task.fire_to_irq_ns = ACTIVE_FIRE_TO_IRQ_NS.load(Ordering::Acquire);
    task.irq_to_worker_resume_ns = ACTIVE_IRQ_TO_RESUME_NS.load(Ordering::Acquire);
    task.tdma_irq_count = ACTIVE_TDMA_IRQ_COUNT.load(Ordering::Acquire);
}

fn publish_completed_task(task: TpuTask) {
    {
        let mut done = DONE_LIST.lock();
        done.push_back(task);
        while done.len() > DONE_LIST_MAX {
            let dropped = done.pop_front();
            if let Some(t) = dropped {
                warn!(
                    "[TPU] done list full, dropping orphaned result (tid={}, seq_no={})",
                    t.tid, t.seq_no
                );
            }
        }
    }
    DONE_WQ.notify_all(true);
}

/// 常驻 worker 线程主循环（对应 Linux `work_thread_main`）。
///
/// 串行取任务、调用 `run_one` 跑硬件、回填结果到 `DONE_LIST` 并唤醒等待者。
/// 单 worker 保证硬件串行访问，无需额外 run 锁。
fn tpu_worker(hw: Arc<Sg2002Tpu>) {
    info!("[TPU] worker thread started");
    loop {
        // 取一个任务；队列空则睡在 TASK_WQ 上让出 CPU。
        // 注意：拿到 guard 后立即在表达式内释放，绝不持锁调用 wait*。
        let mut task = loop {
            if let Some(task) = TASK_LIST.lock().pop_front() {
                break task;
            }
            TASK_WQ.wait_until(|| !TASK_LIST.lock().is_empty());
        };

        // 跑硬件：内部等待 TDMA 完成时经注入的 tpu_wait_irq 睡眠让出 CPU。
        run_tpu_task(&hw, &mut task);

        // 入队完成结果并唤醒等待者。若提交线程从不 wait（或 wait 前退出），其
        // 完成项会滞留并攥住 `Arc<IonBuffer>` 永不释放——故对 DONE_LIST 设上限，
        // 超限时丢弃最旧项（连带释放其 buffer 强引用），对应原 Linux 驱动的
        // `cvi_tpu_cleanup_done_list`。
        publish_completed_task(task);
    }
}

impl TpuDevice {
    pub fn probe() -> Option<Self> {
        let resource = TpuResource::probe()?;
        let hw = {
            let (tdma, tiu) = map_tpu_mmio(resource)?;
            Arc::new(unsafe { Sg2002Tpu::from_vaddr(tdma, tiu) })
        };
        Some(Self::setup(hw, resource))
    }

    /// 公共初始化：注入等待函数、注册中断、启动 worker 线程。
    fn setup(hw: Arc<Sg2002Tpu>, resource: TpuResource) -> Self {
        hw.set_wait_irq_fn(tpu_wait_irq);
        hw.set_tdma_timing_callbacks(mark_tdma_fire, mark_tdma_worker_resume);
        if let Err(err) = hw.init() {
            warn!("[TPU] init failed: {:?}", err);
        }
        let irq_registration = register_tpu_irq(resource.irq, &hw);
        info!(
            "[TPU] resource tdma=[{:#x}, +{:#x}) tiu=[{:#x}, +{:#x}) irq={:?} irq_wait={} \
             source=fdt",
            resource.tdma_paddr,
            resource.tdma_size,
            resource.tiu_paddr,
            resource.tiu_size,
            resource.irq,
            irq_registration.is_some(),
        );

        // 发布硬件指针供 tpu_wait_irq 读取中断标志。异步 A/B 模式才需要
        // 常驻 worker；直接模式由提交线程执行 run_one。
        HW_PTR.store(Arc::as_ptr(&hw) as *mut Sg2002Tpu, Ordering::Release);
        if !TPU_DIRECT_EXECUTION
            && WORKER_SPAWNED
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            let worker_hw = hw.clone();
            ax_task::spawn_with_name(move || tpu_worker(worker_hw), String::from("tpu-worker"));
        }
        info!(
            "[TPU] execution path={}",
            if TPU_DIRECT_EXECUTION {
                "direct"
            } else {
                "worker"
            }
        );

        Self {
            hw,
            resource,
            irq_registration,
        }
    }

    /// 提交 DMA buffer 任务：解析 fd → 入队 → 唤醒 worker → 立即返回。
    fn submit_dmabuf(&self, arg: usize) -> Result<usize, TpuError> {
        // 从用户空间复制参数，不能直接解引用用户指针。
        let submit_arg = unsafe {
            (arg as *const CviSubmitDmaArg)
                .vm_read_uninit()
                .map_err(|_| TpuError::InvalidDmabuf)?
                .assume_init()
        };

        debug!(
            "[TPU] submit dmabuf: fd={}, seq_no={}",
            submit_arg.fd, submit_arg.seq_no
        );
        if self.irq_registration.is_none() {
            warn!("[TPU] TDMA IRQ {:?} not registered", self.resource.irq);
        }

        // 从文件描述符获取 IonBufferFile
        let fd = submit_arg.fd;
        let file = get_file_like(fd).map_err(|_| {
            error!("[TPU] Failed to get file for fd={}", fd);
            TpuError::InvalidDmabuf
        })?;

        // 尝试转换为 IonBufferFile (使用 downcast_arc)
        let ion_file: Arc<IonBufferFile> = file.downcast_arc::<IonBufferFile>().map_err(|_| {
            error!("[TPU] fd={} is not an IonBufferFile", fd);
            TpuError::InvalidDmabuf
        })?;

        // 获取底层 Ion buffer。clone 一份 Arc 强引用随任务存活，确保 worker
        // 访问 DMA 内存期间（即使用户已 close fd）物理页不被回收。
        let buffer = ion_file.buffer().clone();
        debug!(
            "[TPU] dmabuf info: handle={}, size={}, paddr=0x{:x}",
            buffer.handle.as_u32(),
            buffer.size,
            buffer.dma_info.bus_addr.as_u64()
        );

        let kind = classify_tpu_task(&buffer);
        let mut task = TpuTask {
            tid: ax_task::current().id().as_u64(),
            seq_no: submit_arg.seq_no,
            vaddr: buffer.dma_info.cpu_addr.as_ptr() as usize,
            paddr: buffer.dma_info.bus_addr.as_u64(),
            _buffer: buffer,
            ret: 0,
            submit_ns: timing_now_ns(),
            worker_start_ns: 0,
            worker_done_ns: 0,
            fire_to_irq_ns: 0,
            irq_to_worker_resume_ns: 0,
            tdma_irq_count: 0,
            kind,
        };

        if TPU_DIRECT_EXECUTION {
            // Keep the vendor submit/wait ABI, but remove both scheduler
            // hand-offs for the common single-stream cviruntime path.  This
            // blocking mutex preserves the old worker's hardware
            // serialisation and may sleep while another caller owns the TPU.
            let _run_guard = TPU_RUN_LOCK.lock();
            run_tpu_task(&self.hw, &mut task);
            publish_completed_task(task);
        } else {
            // Asynchronous compatibility path used for A/B measurements.
            TASK_LIST.lock().push_back(task);
            TASK_WQ.notify_one(true);
        }

        Ok(0)
    }

    /// 等待 DMA buffer 完成：按 `(tid, seq_no)` 睡 `DONE_WQ`，被 worker 唤醒后
    /// 取结果。用调用线程 tid 与用户 seq_no 组成复合键，隔离跨进程/线程的相同
    /// seq_no——否则两个进程都从 seq 0 开始会互相取走对方的完成项。
    fn wait_dmabuf(&self, arg: usize) -> Result<usize, TpuError> {
        let user_pointer = arg as *mut CviWaitDmaArg;
        let mut wait_arg = unsafe {
            user_pointer
                .vm_read_uninit()
                .map_err(|_| TpuError::InvalidDmabuf)?
                .assume_init()
        };
        let seq_no = wait_arg.seq_no;
        let tid = ax_task::current().id().as_u64();

        // 睡在 DONE_WQ 上直到对应 (tid, seq_no) 出现在完成队列（或超时）。
        // wait_timeout_until 睡前复检谓词，等价 Linux wait_event。
        let timed_out = DONE_WQ.wait_timeout_until(TPU_WAIT_TIMEOUT, || {
            DONE_LIST
                .lock()
                .iter()
                .any(|t| t.tid == tid && t.seq_no == seq_no)
        });

        // 取出该任务结果（即使超时也再查一次，处理临界完成）。
        let found = {
            let mut done = DONE_LIST.lock();
            done.iter()
                .position(|t| t.tid == tid && t.seq_no == seq_no)
                .map(|idx| done.remove(idx).unwrap())
        };

        match found {
            Some(task) => {
                wait_arg.ret = task.ret;
                user_pointer
                    .vm_write(wait_arg)
                    .map_err(|_| TpuError::InvalidDmabuf)?;
                record_tpu_timing(&task, timing_now_ns());
                // Match the vendor ABI: ioctl itself succeeds once a result
                // was found; hardware failure is returned in `wait_arg.ret`.
                // Returning an ioctl error here makes cviruntime retry forever.
                Ok(0)
            }
            None => {
                wait_arg.ret = -1;
                user_pointer
                    .vm_write(wait_arg)
                    .map_err(|_| TpuError::InvalidDmabuf)?;
                warn!(
                    "[TPU] wait dmabuf: (tid={}, seq_no={}) not found (timed_out={})",
                    tid, seq_no, timed_out
                );
                Err(TpuError::Timeout)
            }
        }
    }

    /// 刷新 DMA buffer 缓存 (通过物理地址)
    fn cache_flush(&self, arg: usize) -> Result<usize, TpuError> {
        let flush_arg = unsafe {
            (arg as *const CviCacheOpArg)
                .vm_read_uninit()
                .map_err(|_| TpuError::InvalidDmabuf)?
                .assume_init()
        };
        self.hw.cache_flush_paddr(flush_arg.paddr, flush_arg.size)?;
        Ok(0)
    }

    /// 无效化 DMA buffer 缓存 (通过物理地址)
    fn cache_invalidate(&self, arg: usize) -> Result<usize, TpuError> {
        let invalidate_arg = unsafe {
            (arg as *const CviCacheOpArg)
                .vm_read_uninit()
                .map_err(|_| TpuError::InvalidDmabuf)?
                .assume_init()
        };
        self.hw
            .cache_invalidate_paddr(invalidate_arg.paddr, invalidate_arg.size)?;
        Ok(0)
    }

    /// 刷新 DMA buffer 缓存 (通过 fd)
    fn dmabuf_flush_fd(&self, arg: usize) -> Result<usize, TpuError> {
        let fd = (arg as *const i32)
            .vm_read()
            .map_err(|_| TpuError::InvalidDmabuf)?;
        debug!("TPU dmabuf flush fd: {}", fd);
        let buffer = self.lookup_ion_buffer(fd)?;
        let paddr = buffer.dma_info.bus_addr.as_u64();
        let size = buffer.size as u64;
        self.hw.cache_flush_paddr(paddr, size)?;
        debug!("Flushed buffer: paddr=0x{:x}, size={}", paddr, size);
        Ok(0)
    }

    /// 无效化 DMA buffer 缓存 (通过 fd)
    fn dmabuf_invld_fd(&self, arg: usize) -> Result<usize, TpuError> {
        let fd = (arg as *const i32)
            .vm_read()
            .map_err(|_| TpuError::InvalidDmabuf)?;
        debug!("TPU dmabuf invalidate fd: {}", fd);
        let buffer = self.lookup_ion_buffer(fd)?;
        let paddr = buffer.dma_info.bus_addr.as_u64();
        let size = buffer.size as u64;
        self.hw.cache_invalidate_paddr(paddr, size)?;
        Ok(0)
    }

    /// 把用户传入的 fd 解析为底层 [`sg2002_tpu::ion::IonBuffer`]。
    ///
    /// fd（由 `add_file_like` 分配的文件描述符）与 Ion 内部 handle（来自
    /// `IonHandle` 的全局递增计数）属于两个独立的编号空间，不能直接互相替代。
    /// 因此这里走和 `submit_dmabuf` 一致的路径：fd → `IonBufferFile` →
    /// 持有的 `Arc<IonBuffer>`。
    fn lookup_ion_buffer(&self, fd: i32) -> Result<Arc<IonBuffer>, TpuError> {
        let file = get_file_like(fd).map_err(|err| {
            error!("[TPU] failed to get file for fd={}: {:?}", fd, err);
            TpuError::InvalidDmabuf
        })?;
        let ion_file: Arc<IonBufferFile> = file.downcast_arc::<IonBufferFile>().map_err(|_| {
            error!("[TPU] fd={} is not an IonBufferFile", fd);
            TpuError::InvalidDmabuf
        })?;
        Ok(ion_file.buffer().clone())
    }
}

impl DeviceOps for TpuDevice {
    fn read_at(&self, _buf: &mut [u8], _offset: u64) -> axfs_ng_vfs::VfsResult<usize> {
        Ok(0)
    }

    fn write_at(&self, _buf: &[u8], _offset: u64) -> axfs_ng_vfs::VfsResult<usize> {
        Ok(0)
    }

    fn ioctl(&self, cmd: u32, arg: usize) -> axfs_ng_vfs::VfsResult<usize> {
        debug!("TPU ioctl: cmd=0x{:x}, arg=0x{:x}", cmd, arg);

        let result = match cmd {
            CVITPU_SUBMIT_DMABUF => self.submit_dmabuf(arg),
            CVITPU_DMABUF_FLUSH_FD => self.dmabuf_flush_fd(arg),
            CVITPU_DMABUF_INVLD_FD => self.dmabuf_invld_fd(arg),
            CVITPU_DMABUF_FLUSH => self.cache_flush(arg),
            CVITPU_DMABUF_INVLD => self.cache_invalidate(arg),
            CVITPU_WAIT_DMABUF => self.wait_dmabuf(arg),
            CVITPU_PIO_MODE => {
                warn!("TPU PIO mode not implemented");
                Ok(0)
            }
            CVITPU_LOAD_TEE | CVITPU_SUBMIT_TEE | CVITPU_UNLOAD_TEE => {
                warn!("TPU TEE operations not supported");
                Err(TpuError::NotInitialized)
            }
            _ => {
                warn!("Unknown TPU ioctl command: 0x{:x}", cmd);
                Err(TpuError::NotInitialized)
            }
        };

        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                error!("TPU ioctl error: {:?}", e);
                Err(ax_errno::AxError::Unsupported)
            }
        }
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }
}
