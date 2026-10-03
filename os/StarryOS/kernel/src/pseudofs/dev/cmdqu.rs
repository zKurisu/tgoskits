//! SG2002 跨核命令队列（cmdqu）设备。
//!
//! 这一层负责把 [`sg2002_cmdqu`] 提供的硬件抽象接进 StarryOS：从设备树探测寄存器
//! 与中断、建立非缓存映射、在中断里把消息取进队列，并通过 `/dev/cvi-rtos-cmdqu`
//! 暴露给用户态。
//!
//! 与厂商驱动的两点差异值得注意：一是"等待哪条应答"由调用方显式指定（厂商实现
//! 把等待命令号写死成发送命令号，导致 0x51 只能等来静默超时）；二是所有等待都在
//! `sleep` 之后重新检查队列，不会在持有锁的情况下睡眠。

use alloc::sync::Arc;
use core::{
    any::Any,
    sync::atomic::{AtomicU8, Ordering},
    time::Duration,
};

use ax_memory_addr::{PhysAddr, PhysAddrRange};
use ax_lazyinit::LazyInit;
use axpoll::{IoEvents, Pollable, SharedRegistrationSink};
use axpoll_set::PollSet;
use axfs_ng_vfs::{NodeFlags, VfsError, VfsResult};
use ax_std::os::arceos::{
    task as scheduler,
    task::{
        sync::{
            WaitQueue,
            irq::{IrqWaitCell, IrqWaitRegistration},
        },
        thread::ThreadId,
    },
};
use bytemuck::NoUninit;
use sg2002_cmdqu::{
    Envelope, Mailbox,
    shm::{SYNC_HOST_OFFSET, SYNC_RTOS_OFFSET, ShmHeader, SyncHostBlock, SyncRtosBlock},
};

use super::{IrqRegistration, irq_service::complete_irq_service_cycle, request_shared_disabled};
use crate::{
    mm::{VmMutPtr, VmPtr},
    pseudofs::{DeviceMmap, DeviceOps},
    sync::IrqMutex,
    task::UserTaskRef,
};

/// 设备树里的 mailbox 节点 compatible 与中断名。
const COMPATIBLE: &[&str] = &["cvitek,rtos_cmdqu"];
const IRQ_NAME: &str = "mailbox";
/// 跨核共享缓冲区的设备树 compatible（写在 `reserved-memory` 子节点上）。
///
/// 该节点同时起到两个作用：告诉内核不要把这些页交给分配器，以及告诉本驱动
/// 共享区的物理地址与大小。
const SHM_COMPATIBLE: &[&str] = &["cvitek,cmdqu-shm"];
/// 接收队列上限，超出时丢弃最旧消息并计数。
const MAX_RX_QUEUE: usize = 64;

/// 硬中断只能通过这个单元唤醒"一个固定服务线程"：`WaitQueue` 的唤醒 API
/// 在硬中断上下文会 panic，源码注释与 `pseudofs/dev/kpu.rs` 都明确要求这种两段式。
static IRQ_NOTIFY: IrqWaitCell = IrqWaitCell::new();
/// 队列就绪通知，由服务线程在任务上下文扇出给真正的等待者。
static RX_READY: WaitQueue = WaitQueue::new();
/// 服务线程自身的挂起队列。
static SERVICE_PARK: WaitQueue = WaitQueue::new();
/// `poll`/`epoll` 观察者：队列就绪时由服务线程在任务上下文唤醒。
static POLL_WAITERS: PollSet = PollSet::new();
/// 诊断计数：`poll` 观察者注册到本设备的次数。
static POLL_REGISTRATIONS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
/// 诊断计数：服务线程扇出时唤醒 `poll` 观察者的次数。
static POLL_WAKES: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static SERVICE_WAITER: LazyInit<ServiceWaiter> = LazyInit::new();
static SERVICE_STATE: AtomicU8 = AtomicU8::new(SERVICE_STOPPED);

const SERVICE_STOPPED: u8 = 0;
const SERVICE_STARTING: u8 = 1;
const SERVICE_STARTED: u8 = 2;

/// 固定服务线程的注册信息：中断用 `IrqWaitCell` 直接唤醒它。
struct ServiceWaiter {
    owner: ThreadId,
    registration: IrqWaitRegistration,
}

/// `ioctl` 命令编号，编码规则与 Linux 的 `_IOW('r', nr, unsigned long)` 一致。
/// 前四个沿用厂商设备节点的编号，方便复用既有用户态程序。
///
/// `0x10` 起的四个是**本驱动自加的调试接口**（收信、发送并等待、累计计数、现场体检），
/// 不属于厂商节点那套稳定 ABI，也不承诺跨版本兼容：厂商只定义 `1..=5`
/// （SEND / REQUEST / REQUEST_FREE / SEND_WAIT / SEND_WAKEUP），`0x10` 以上在协议里是空的，
/// 因此不会与厂商程序冲突。
///
/// 2026-10-03 的决定是**保留为 ioctl，不挪进 debugfs**，理由是这两条：
/// 1. 现场排查真正需要的正是"一次系统调用拿到小核在不在跑、队列有没有丢包"，
///    而 `debugfs` 要先挂载、要多依赖一层文件系统语义，偏偏这些接口的使用场景是
///    "系统刚起来、什么都不能假设"的时候；`tools/cmdqu_test` 与板测用例
///    `cmdqu-selftest` 也都直接依赖它们。
/// 2. 它们不占厂商编号段，也不会让厂商程序误用。
///
/// 若将来把驱动上游化、需要收敛公共接口，再把这四条挪到 `/sys/kernel/debug` 下，
/// 只把 `RECV` / `EXCHANGE` 留在设备节点上。
pub const CMDQU_SEND: u32 = iow(1);
pub const CMDQU_REQUEST: u32 = iow(2);
pub const CMDQU_REQUEST_FREE: u32 = iow(3);
pub const CMDQU_SEND_WAIT: u32 = iow(4);
/// 新增：一次调用完成"发送 + 等待指定应答"。
pub const CMDQU_EXCHANGE: u32 = iow(0x11);
/// 新增：从接收队列取一条消息。
pub const CMDQU_RECV: u32 = iow(0x10);
/// 新增：读取累计计数。
pub const CMDQU_STATS: u32 = iow(0x12);
/// 新增：现场体检（小核是否在跑、槽位与中断状态）。
pub const CMDQU_DIAG: u32 = iow(0x13);

/// 诊断时需要覆盖 PC 监视寄存器（0x1070），所以映射窗口取 8 KiB。
const MAPPED_WINDOW: usize = 0x2000;

const IOC_WRITE: u32 = 1;
const IOC_SIZE: u32 = 8; // sizeof(unsigned long)
const IOC_TYPE: u32 = b'r' as u32;

const fn iow(nr: u32) -> u32 {
    (IOC_WRITE << 30) | (IOC_SIZE << 16) | (IOC_TYPE << 8) | nr
}

/// 用户态传进来的 8 字节信封（两次 32 位读，与槽位布局一致）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, NoUninit)]
pub struct RawEnvelope {
    /// 头部字：`ip_id | cmd_id<<8 | block<<15 | linux_valid<<16 | rtos_valid<<24`。
    pub header: u32,
    /// 载荷指针或小整数参数。
    pub param_ptr: u32,
}

impl RawEnvelope {
    /// 用户态视角的信封：只暴露 ip_id/cmd_id/block/param_ptr，方向位由内核填。
    pub fn to_envelope(self) -> Envelope {
        let mut env = Envelope::from_words(self.header, self.param_ptr);
        // 用户态不允许伪造方向位，统一由内核按"大核发出"处理。
        env.linux_valid = true;
        env.rtos_valid = false;
        env
    }
}

/// 一次"发送 + 等待指定应答"的请求。
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, NoUninit)]
pub struct CmdquExchange {
    /// 发送用字段。
    pub ip_id: u8,
    pub cmd_id: u8,
    pub block: u8,
    pub flags: u8,
    pub param_ptr: u32,
    /// 等待用字段：期望的应答 ip_id / cmd_id 与超时。
    pub wait_ip_id: u8,
    pub wait_cmd_id: u8,
    pub reserved: u16,
    pub timeout_ms: u32,
    /// 输出：应答的 param_ptr。
    pub reply_param_ptr: u32,
    /// 输出：0 = 拿到应答，2 = 按调用方要求没有等待（`timeout_ms == 0`）。
    ///
    /// **超时不在这里表达**：等待超时时 ioctl 直接返回 `-ETIME`，并且不回填本结构体
    /// （出错时输出字段无定义，这是 Linux 的惯用约定）。曾经在超时同时写回
    /// `status = 1`，于是同一个事实有 errno 与字段两条口径，用户态还得先读回结构体
    /// 才知道发生了什么——2026-10-03 收窄成单一口径。
    pub status: u32,
}

impl CmdquExchange {
    /// `flags` 位 0：只等待、不发送。
    pub const FLAG_SKIP_SEND: u8 = 1 << 0;
}

/// 累计计数，供 `CMDQU_STATS` 读取。
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, NoUninit)]
pub struct CmdquStats {
    pub tx: u64,
    pub rx: u64,
    pub irq: u64,
    pub dropped: u64,
    /// `poll` 观察者注册次数（诊断用）。
    pub poll_registrations: u64,
    /// 服务线程唤醒 `poll` 观察者的次数（诊断用）。
    pub poll_wakes: u64,
}

/// 现场体检结果，供 `CMDQU_DIAG` 返回。
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, NoUninit)]
pub struct CmdquDiag {
    /// 小核程序计数器两次采样：值不同说明它在执行。
    pub pc_a: u32,
    pub pc_b: u32,
    /// 本核待处理中断位图与两侧使能位。
    pub pending: u8,
    pub own_enable: u8,
    pub peer_enable: u8,
    pub reserved: u8,
    /// 8 个槽位的两个 32 位字，按槽位顺序展开。
    pub slots: [u32; 16],
    /// ION 窗口抽样：16 个采样页里有多少页存在非零数据。
    pub ion_nonzero_pages: u32,
    /// 第一个采样页里出现的第一个非零字。
    pub ion_first_word: u32,
    /// 全部采样字的 FNV-1a 校验和：两次采样之间变化说明有人正在写这段内存。
    pub ion_checksum: u32,
    /// 跨核共享缓冲区的物理地址（0 表示设备树里没有声明）。
    pub shm_paddr: u32,
    /// 共享缓冲区大小（字节）。
    pub shm_size: u32,
    /// 共享缓冲区自检：0 = 未映射，1 = 读写回环通过，2 = 失败。
    pub shm_selftest: u32,
    /// 共享区头部状态：0 = 未映射，1 = 已有兼容头部，2 = 本次写入并校验通过，3 = 校验失败。
    pub shm_header: u32,
}

/// 已映射的跨核共享缓冲区。
struct SharedBuffer {
    paddr: usize,
    size: usize,
    /// 非缓存映射后的虚拟地址。
    vaddr: usize,
}

/// 厂商内存布局给小核的多媒体缓冲窗口（`ION_ADDR` 起 75 MiB）。
///
/// 这段落在 StarryOS 的可用内存范围内，因此需要确认小核是否真的在用它：
/// 如果整段都是零，说明当前固件在 StarryOS 下没有使用它，页分配器可以随意使用；
/// 若出现非零数据，就必须在设备树里给这段加保留，否则会被写坏。
const ION_BASE: usize = 0x8b30_0000;
const ION_WINDOW: usize = 75 * 1024 * 1024;
const ION_SAMPLE_PAGES: usize = 16;
const ION_SAMPLE_WORDS: usize = 256; // 每页抽样 1 KiB

/// 定长环形队列。
///
/// 中断处理程序在持有 IRQ-save 自旋锁期间不允许调用分配器，所以这里不用
/// `VecDeque`，改用固定容量数组；满了就覆盖最旧的一条并计数。
struct EnvelopeQueue<const N: usize> {
    slots: [Option<Envelope>; N],
    head: usize,
    len: usize,
    dropped: u64,
}

impl<const N: usize> EnvelopeQueue<N> {
    const fn new() -> Self {
        Self {
            slots: [None; N],
            head: 0,
            len: 0,
            dropped: 0,
        }
    }

    fn push(&mut self, envelope: Envelope) {
        if self.len == N {
            self.slots[self.head] = Some(envelope);
            self.head = (self.head + 1) % N;
            self.dropped += 1;
            return;
        }
        let index = (self.head + self.len) % N;
        self.slots[index] = Some(envelope);
        self.len += 1;
    }

    fn pop(&mut self) -> Option<Envelope> {
        if self.len == 0 {
            return None;
        }
        let envelope = self.slots[self.head].take();
        self.head = (self.head + 1) % N;
        self.len -= 1;
        envelope
    }

    const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 取出第一条 `(ip_id, cmd_id)` 匹配的消息，其余保持顺序。
    fn remove_matching(&mut self, ip_id: u8, cmd_id: u8) -> Option<Envelope> {
        for offset in 0..self.len {
            let index = (self.head + offset) % N;
            let Some(envelope) = self.slots[index] else {
                continue;
            };
            if envelope.ip_id != ip_id || envelope.cmd_id != cmd_id {
                continue;
            }
            // 后面的元素依次前移，保持 FIFO 顺序
            for shift in offset..self.len - 1 {
                let from = (self.head + shift + 1) % N;
                let to = (self.head + shift) % N;
                self.slots[to] = self.slots[from].take();
            }
            let last = (self.head + self.len - 1) % N;
            self.slots[last] = None;
            self.len -= 1;
            return Some(envelope);
        }
        None
    }

    /// 队列里是否存在匹配的消息（不移除）。
    fn contains_matching(&self, ip_id: u8, cmd_id: u8) -> bool {
        (0..self.len).any(|offset| {
            let index = (self.head + offset) % N;
            matches!(self.slots[index], Some(envelope)
                if envelope.ip_id == ip_id && envelope.cmd_id == cmd_id)
        })
    }
}

impl<const N: usize> Default for EnvelopeQueue<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
struct State {
    rx: EnvelopeQueue<MAX_RX_QUEUE>,
    tx: u64,
    rx_total: u64,
    irq: u64,
    /// 中断注册句柄，随设备存活；`None` 表示未接中断、只能用轮询。
    _irq: Option<IrqRegistration>,
}

impl State {
    fn stats(&self) -> CmdquStats {
        CmdquStats {
            tx: self.tx,
            rx: self.rx_total,
            irq: self.irq,
            dropped: self.rx.dropped,
            poll_registrations: POLL_REGISTRATIONS.load(Ordering::Acquire),
            poll_wakes: POLL_WAKES.load(Ordering::Acquire),
        }
    }
}

/// `/dev/cvi-rtos-cmdqu` 对应的设备。
pub struct CmdquDevice {
    mailbox: Mailbox,
    state: IrqMutex<State>,
    /// 设备树声明了共享区才存在；没有声明就不映射，避免与分析器争内存。
    shared: Option<SharedBuffer>,
    /// 共享区头部状态（见 `SharedBuffer::init_header` 的返回值）。
    shm_header: u32,
}

impl CmdquDevice {
    /// 从设备树探测寄存器与中断，建立非缓存映射并注册中断。
    pub fn probe() -> Option<Arc<Self>> {
        let (paddr, irq) = probe_fdt()?;
        let vaddr = match ax_mm::iomap_uncached(PhysAddr::from_usize(paddr), MAPPED_WINDOW) {
            Ok(vaddr) => vaddr.as_usize(),
            Err(err) => {
                warn!("[cmdqu] iomap 0x{paddr:08x} 失败: {err:?}");
                return None;
            }
        };
        info!(
            "[cmdqu] mailbox 寄存器 0x{paddr:08x} -> 0x{vaddr:016x}（非缓存，{} KiB），irq={irq:?}",
            MAPPED_WINDOW / 1024
        );

        let shared = probe_shared_buffer();
        let shm_header = shared.as_ref().map(SharedBuffer::init_header).unwrap_or(0);
        let device = Arc::new(Self {
            // SAFETY: vaddr 来自 iomap_uncached，是一段可读写的非缓存 MMIO 映射。
            mailbox: unsafe { Mailbox::new(vaddr) },
            state: IrqMutex::new(State::default()),
            shared,
            shm_header,
        });
        if let Some(shared) = device.shared.as_ref() {
            info!(
                "[cmdqu] 跨核共享缓冲 0x{:08x} + {} KiB -> 0x{:016x}（非缓存），头部状态={}",
                shared.paddr,
                shared.size / 1024,
                shared.vaddr,
                device.shm_header
            );
        } else {
            warn!("[cmdqu] 设备树里没有 cvitek,cmdqu-shm 节点，跳过共享缓冲映射");
        }

        // 中断注册需要 Arc<Self>，因此放在构造之后。
        let handler_device = Arc::clone(&device);
        match request_shared_disabled(irq, move |_| handler_device.handle_irq()) {
            Ok(registration) => match registration.enable() {
                Ok(()) => {
                    device.state.lock()._irq = Some(registration);
                    // 中断唤醒链需要服务线程就位，否则等待者只能靠条件闭包自愈。
                    let service_ok = start_irq_service();
                    info!("[cmdqu] 已注册并启用 mailbox 中断（服务线程={service_ok}）");
                }
                Err(err) => warn!("[cmdqu] 启用中断失败: {err:?}（将退化为轮询）"),
            },
            Err(err) => warn!("[cmdqu] 注册中断失败: {err:?}（将退化为轮询）"),
        }

        Some(device)
    }

    /// 设备节点名。
    pub const DEVICE_NAME: &'static str = "cvi-rtos-cmdqu";

    /// 中断处理：把本轮所有待处理槽位取进队列。这里不做任何打印。
    fn handle_irq(&self) -> ax_runtime::hal::irq::IrqReturn {
        let mut state = self.state.lock();
        state.irq += 1;
        while let Some((_slot, envelope)) = self.mailbox.take_pending() {
            state.rx.push(envelope);
            state.rx_total += 1;
        }
        drop(state);
        // 硬中断只唤醒固定服务线程，由后者在任务上下文里做等待队列扇出。
        let _ = IRQ_NOTIFY.notify();
        ax_runtime::hal::irq::IrqReturn::Handled
    }

    /// 把一条信封写进空槽并拉门铃。
    fn send(&self, envelope: Envelope) -> VfsResult<usize> {
        // 子系统号与命令号的取值范围由协议硬性规定，越界的消息对端会直接丢弃，
        // 在本地拦下比让它到对端再被拒绝更容易定位。
        envelope.validate().map_err(|_| VfsError::InvalidInput)?;
        let slot = self
            .mailbox
            .send(envelope)
            .map_err(|_| VfsError::ResourceBusy)?;
        self.state.lock().tx += 1;
        Ok(slot)
    }

    /// 从队列里取一条匹配 `(ip_id, cmd_id)` 的应答。
    fn take_matching(&self, ip_id: u8, cmd_id: u8) -> Option<Envelope> {
        self.state.lock().rx.remove_matching(ip_id, cmd_id)
    }

    /// 队列里是否已有匹配的应答（不移除）。
    fn has_matching(&self, ip_id: u8, cmd_id: u8) -> bool {
        self.state.lock().rx.contains_matching(ip_id, cmd_id)
    }

    /// 是否完全没有待处理消息：软件队列为空**且**硬件槽位里也没有待搬的报文。
    ///
    /// 只看软件队列是不够的——中断处理程序可能还没跑（或消息是在两次中断之间
    /// 到达的），此时报文躺在 mailbox 槽位里，若只判队列就会出现"数据已到却报
    /// 不可读、`read` 返回 EAGAIN、`poll` 睡到超时"的假阴性。
    fn rx_is_empty(&self) -> bool {
        if !self.state.lock().rx.is_empty() {
            return false;
        }
        self.mailbox.pending_mask() == 0
    }

    /// 没有中断（或中断没来得及跑）时，把硬件里躺着的消息搬进队列。
    fn drain_hardware(&self) {
        while let Some((_slot, envelope)) = self.mailbox.take_pending() {
            let mut state = self.state.lock();
            state.rx.push(envelope);
            state.rx_total += 1;
        }
    }

    /// 阻塞等待指定应答：先看队列，再睡在 `RX_READY` 上等中断唤醒。
    ///
    /// 条件闭包里也会顺手把硬件里躺着的消息搬进队列，这样即使中断路径失效
    /// 也能自愈；闭包在等待任务上下文执行，不持任何锁。
    fn wait_for(&self, ip_id: u8, cmd_id: u8, timeout_ms: u32) -> Option<Envelope> {
        self.drain_hardware();
        if let Some(envelope) = self.take_matching(ip_id, cmd_id) {
            return Some(envelope);
        }
        let _timed_out = RX_READY.wait_timeout_until(
            Duration::from_millis(timeout_ms.max(1) as u64),
            || {
                self.drain_hardware();
                self.has_matching(ip_id, cmd_id)
            },
        );
        self.take_matching(ip_id, cmd_id)
    }

    fn read_raw_envelope(&self, current: &UserTaskRef, arg: usize) -> VfsResult<RawEnvelope> {
        if arg == 0 {
            return Err(VfsError::BadAddress);
        }
        let raw =
            (arg as *const RawEnvelope).vm_read_uninit(current).map_err(|_| VfsError::BadAddress)?;
        Ok(unsafe { raw.assume_init() })
    }

    fn write_struct<T: NoUninit>(
        &self,
        current: &UserTaskRef,
        arg: usize,
        value: T,
    ) -> VfsResult<()> {
        if arg == 0 {
            return Err(VfsError::BadAddress);
        }
        (arg as *mut T)
            .vm_write(current, value)
            .map_err(|_| VfsError::BadAddress)?;
        Ok(())
    }

    fn handle_send(&self, current: &UserTaskRef, arg: usize) -> VfsResult<usize> {
        let raw = self.read_raw_envelope(current, arg)?;
        self.send(raw.to_envelope())
    }

    /// 现场体检：PC 采样、中断与使能位、八个槽位内容。
    fn handle_diag(&self, current: &UserTaskRef, arg: usize) -> VfsResult<usize> {
        let mut diag = CmdquDiag::default();
        diag.pc_a = self.mailbox.pc_monitor();
        for _ in 0..200_000 {
            core::hint::spin_loop();
        }
        diag.pc_b = self.mailbox.pc_monitor();
        diag.pending = self.mailbox.pending_mask();
        diag.peer_enable = self.mailbox.peer_enable_mask();
        diag.own_enable = self.mailbox.own_enable_mask();
        for slot in 0..8 {
            let (w0, w1) = self.mailbox.slot_words(slot);
            diag.slots[slot * 2] = w0;
            diag.slots[slot * 2 + 1] = w1;
        }
        let (ion_nonzero_pages, ion_first_word, ion_checksum) = sample_ion_window();
        diag.ion_nonzero_pages = ion_nonzero_pages;
        diag.ion_first_word = ion_first_word;
        diag.ion_checksum = ion_checksum;
        if let Some(shared) = self.shared.as_ref() {
            diag.shm_paddr = shared.paddr as u32;
            diag.shm_size = shared.size as u32;
            diag.shm_selftest = shared.selftest();
            diag.shm_header = self.shm_header;
        }
        // 顺带把硬件里已经躺着的消息收进队列，避免诊断本身"吃掉"消息
        self.drain_hardware();
        self.write_struct(current, arg, diag)?;
        info!(
            "[cmdqu] diag: pc=0x{:08x}/0x{:08x} pending=0x{:02x} own_en=0x{:02x} peer_en=0x{:02x} slot0=0x{:08x} ion非零页={}/{} sum=0x{:08x}",
            diag.pc_a, diag.pc_b, diag.pending, diag.own_enable, diag.peer_enable,
            diag.slots[0], diag.ion_nonzero_pages, ION_SAMPLE_PAGES, diag.ion_checksum
        );
        Ok(0)
    }

    fn handle_recv(&self, current: &UserTaskRef, arg: usize) -> VfsResult<usize> {
        // 先把硬件里可能还没被中断搬走的报文收进来，再判断队列是否为空。
        self.drain_hardware();
        let mut state = self.state.lock();
        let Some(envelope) = state.rx.pop() else {
            return Err(VfsError::WouldBlock);
        };
        let raw = RawEnvelope {
            header: envelope.header_word(),
            param_ptr: envelope.param_ptr,
        };
        drop(state);
        self.write_struct(current, arg, raw)?;
        Ok(0)
    }

    fn handle_exchange(&self, current: &UserTaskRef, arg: usize) -> VfsResult<usize> {
        let request = (arg as *const CmdquExchange)
            .vm_read_uninit(current)
            .map_err(|_| VfsError::BadAddress)?;
        let mut request = unsafe { request.assume_init() };

        if request.flags & CmdquExchange::FLAG_SKIP_SEND == 0 {
            let envelope = Envelope::request_raw(
                request.ip_id,
                request.cmd_id,
                request.block != 0,
                request.param_ptr,
            );
            self.send(envelope)?;
        }

        if request.timeout_ms == 0 {
            request.status = 2; // 不等待
            self.write_struct(current, arg, request)?;
            return Ok(0);
        }

        match self.wait_for(request.wait_ip_id, request.wait_cmd_id, request.timeout_ms) {
            Some(reply) => {
                request.reply_param_ptr = reply.param_ptr;
                request.status = 0;
                self.write_struct(current, arg, request)?;
                Ok(0)
            }
            None => {
                // 超时只走错误码：调用方看 errno == ETIME 即可，不必先读回结构体。
                // 出错时不给输出字段任何保证，所以这里**不**回填结构体——曾经同时写回
                // `status = 1`，让"超时"有 errno 与字段两条口径（一条错一条对时很难查），
                // 2026-10-03 收窄成单一口径。注意 `timeout_ms == 0`（不等待）是成功路径，
                // 仍然回填 `status = 2`，见上面那个分支。
                Err(VfsError::TimedOut)
            }
        }
    }
}

/// 从设备树的 `reserved-memory` 子节点里找共享缓冲区并建立非缓存映射。
fn probe_shared_buffer() -> Option<SharedBuffer> {
    let (paddr, size) = rdrive::with_fdt(|fdt| {
        fdt.find_compatible(SHM_COMPATIBLE)
            .into_iter()
            .find_map(|node| {
                let reg = node.regs().into_iter().next()?;
                let size = reg.size? as usize;
                Some((reg.address as usize, size))
            })
    })
    .flatten()?;

    match ax_mm::iomap_uncached(PhysAddr::from_usize(paddr), size) {
        Ok(vaddr) => Some(SharedBuffer {
            paddr,
            size,
            vaddr: vaddr.as_usize(),
        }),
        Err(err) => {
            warn!("[cmdqu] 共享缓冲 iomap 0x{paddr:08x}({size:#x}) 失败: {err:?}");
            None
        }
    }
}

impl SharedBuffer {
    /// 初始化（或识别）头部。
    ///
    /// **每次开机都重新写入头部**，而不是复用 DRAM 上遗留的"兼容"头部：
    /// 共享区位于 DRAM，内容跨复位保留，若复用旧头，`ring_head` 会从上一轮的
    /// 值继续累加，而 `ring_tail` 归零，于是几十条陈旧槽位会被当成待消费的新条目
    /// （实板上表现为 seq 在两个开机周期之间跳变）。重新初始化让两个索引归零，
    /// 陈旧槽位自然不会再被引用；小核侧每轮先 invalidate 头部，因此会立刻跟上。
    ///
    /// 返回：0 = 未映射，2 = 本次写入并通过校验，3 = 写入后校验失败。
    fn init_header(&self) -> u32 {
        let header_ptr = self.vaddr as *mut ShmHeader;
        let header = ShmHeader::new(self.size);
        unsafe { core::ptr::write_volatile(header_ptr, header) };
        // 时间基准同步块与头部一起初始化，保证首次读取时字段是确定的。
        // 两个半边各占一条 cache line：大核只写前者，小核只写后者。
        let sync_host_ptr = (self.vaddr + SYNC_HOST_OFFSET) as *mut SyncHostBlock;
        let sync_rtos_ptr = (self.vaddr + SYNC_RTOS_OFFSET) as *mut SyncRtosBlock;
        unsafe {
            core::ptr::write_volatile(sync_host_ptr, SyncHostBlock::new());
            core::ptr::write_volatile(sync_rtos_ptr, SyncRtosBlock::new());
        }
        let written = unsafe { core::ptr::read_volatile(header_ptr) };
        if written.is_compatible() { 2 } else { 3 }
    }

    /// 读写回环自检：写入一个图案、读回、再恢复原值。
    ///
    /// 这段内存由设备树的 `reserved-memory` 保护，当前还没有对端使用，
    /// 所以可以安全地做破坏性很小的自检（只动开头 8 字节并恢复）。
    fn selftest(&self) -> u32 {
        let word0 = self.vaddr as *mut u32;
        let word1 = (self.vaddr + 4) as *mut u32;
        let saved0 = unsafe { core::ptr::read_volatile(word0) };
        let saved1 = unsafe { core::ptr::read_volatile(word1) };

        const PATTERN0: u32 = 0xc0de_1234;
        const PATTERN1: u32 = 0x5a5a_a5a5;
        unsafe {
            core::ptr::write_volatile(word0, PATTERN0);
            core::ptr::write_volatile(word1, PATTERN1);
        }
        let read0 = unsafe { core::ptr::read_volatile(word0) };
        let read1 = unsafe { core::ptr::read_volatile(word1) };
        unsafe {
            core::ptr::write_volatile(word0, saved0);
            core::ptr::write_volatile(word1, saved1);
        }

        if read0 == PATTERN0 && read1 == PATTERN1 {
            1
        } else {
            2
        }
    }
}

/// 抽样检查 ION 窗口里是否有小核留下的数据。
///
/// 该窗口属于 RAM，内核的直接映射已经覆盖，不需要再 iomap；只读不写，
/// 因此即使小核正在使用它也不会被破坏。
fn sample_ion_window() -> (u32, u32, u32) {
    let stride = ION_WINDOW / ION_SAMPLE_PAGES;
    let mut nonzero_pages = 0u32;
    let mut first_word = 0u32;
    let mut checksum = 0x811c_9dc5u32; // FNV-1a 偏移基准

    for page in 0..ION_SAMPLE_PAGES {
        let paddr = ION_BASE + stride * page;
        let vaddr = ax_hal::mem::phys_to_virt(PhysAddr::from_usize(paddr)).as_usize();
        let mut found = false;
        for word in 0..ION_SAMPLE_WORDS {
            let value = unsafe { core::ptr::read_volatile((vaddr + word * 4) as *const u32) };
            for byte in value.to_le_bytes() {
                checksum ^= byte as u32;
                checksum = checksum.wrapping_mul(0x0100_0193);
            }
            if value != 0 {
                found = true;
                if first_word == 0 {
                    first_word = value;
                }
            }
        }
        if found {
            nonzero_pages += 1;
        }
    }
    (nonzero_pages, first_word, checksum)
}

/// 固定 IRQ 服务线程：被硬中断唤醒后，在任务上下文里扇出等待队列。
///
/// 结构照搬 `pseudofs/dev/kpu.rs`：注册 → 挂起 → 确认静默 → 扇出，循环往复。
fn irq_service() {
    let current = scheduler::thread::current::current_thread_handle()
        .unwrap_or_else(|error| panic!("cmdqu IRQ service has no scheduler thread: {error}"));
    let waiter = SERVICE_WAITER.get_or_init(|| ServiceWaiter {
        owner: current.id(),
        registration: IrqWaitRegistration::new(current.wake_handle()),
    });
    assert_eq!(
        waiter.owner,
        current.id(),
        "cmdqu IRQ notifications must be consumed by one fixed service thread"
    );

    loop {
        let registration = IRQ_NOTIFY.register(&waiter.registration);
        let completed = complete_irq_service_cycle(
            registration,
            |token| SERVICE_PARK.wait_until(|| !token.is_attached()),
            || {
                // 两类等待者都在任务上下文被唤醒：ioctl 阻塞等待者与 poll/epoll 观察者。
                RX_READY.notify_all();
                unsafe { POLL_WAITERS.wake(IoEvents::IN) };
                POLL_WAKES.fetch_add(1, Ordering::AcqRel);
            },
        )
        .unwrap_or_else(|error| panic!("cmdqu IRQ waiter could not quiesce: {error}"));
        if !completed {
            panic!("cmdqu IRQ service registration was occupied concurrently");
        }
    }
}

/// 启动服务线程；已经启动或正在启动时返回成功。
fn start_irq_service() -> bool {
    loop {
        match SERVICE_STATE.load(Ordering::Acquire) {
            SERVICE_STARTED => return true,
            SERVICE_STARTING => core::hint::spin_loop(),
            SERVICE_STOPPED => {
                if SERVICE_STATE
                    .compare_exchange(
                        SERVICE_STOPPED,
                        SERVICE_STARTING,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    break;
                }
            }
            _ => unreachable!("invalid cmdqu IRQ service state"),
        }
    }

    match crate::task::kernel_thread_builder("cmdqu-irq-service".into()).spawn(irq_service) {
        Ok(_service) => {
            SERVICE_STATE.store(SERVICE_STARTED, Ordering::Release);
            true
        }
        Err(error) => {
            SERVICE_STATE.store(SERVICE_STOPPED, Ordering::Release);
            warn!("[cmdqu] 启动 IRQ 服务线程失败: {error:?}");
            false
        }
    }
}

/// 从设备树读出 mailbox 的物理地址与中断号。
fn probe_fdt() -> Option<(usize, ax_runtime::hal::irq::IrqId)> {
    // 解析必须在 with_fdt 闭包内完成：BindingIrq 可能借用设备树数据。
    rdrive::with_fdt(|fdt| {
        fdt.find_compatible(COMPATIBLE)
            .into_iter()
            .find_map(|node| {
                let reg = node.regs().into_iter().next()?;
                let binding =
                    ax_driver::binding_irq_from_named_fdt_interrupt(&node, IRQ_NAME).ok()??;
                let irq = ax_runtime::irq::resolve_binding_irq(binding).ok()?;
                Some((reg.address as usize, irq))
            })
    })
    .flatten()
}

impl DeviceOps for CmdquDevice {
    fn read_at(&self, buf: &mut [u8], _offset: u64) -> VfsResult<usize> {
        if buf.len() < 8 {
            return Err(VfsError::InvalidInput);
        }
        // 与 RECV 一致：先搬硬件队列，避免"数据已到却报 EOF/EAGAIN"。
        self.drain_hardware();
        let mut state = self.state.lock();
        let Some(envelope) = state.rx.pop() else {
            return Err(VfsError::WouldBlock);
        };
        drop(state);
        let (header, param) = envelope.words();
        buf[..4].copy_from_slice(&header.to_le_bytes());
        buf[4..8].copy_from_slice(&param.to_le_bytes());
        Ok(8)
    }

    fn write_at(&self, buf: &[u8], _offset: u64) -> VfsResult<usize> {
        if buf.len() < 8 {
            return Err(VfsError::InvalidInput);
        }
        let header = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let param = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        self.send(RawEnvelope { header, param_ptr: param }.to_envelope())?;
        Ok(8)
    }

    fn ioctl(&self, current: &UserTaskRef, cmd: u32, arg: usize) -> VfsResult<usize> {
        match cmd {
            CMDQU_SEND => self.handle_send(current, arg),
            CMDQU_RECV => self.handle_recv(current, arg),
            CMDQU_SEND_WAIT => self.handle_exchange(current, arg),
            CMDQU_EXCHANGE => self.handle_exchange(current, arg),
            CMDQU_STATS => {
                let stats = self.state.lock().stats();
                self.write_struct(current, arg, stats)?;
                Ok(0)
            }
            CMDQU_DIAG => self.handle_diag(current, arg),
            // 厂商节点用这两个命令注册接收回调；本驱动用 read()/recv 语义替代。
            CMDQU_REQUEST | CMDQU_REQUEST_FREE => Err(VfsError::Unsupported),
            _ => Err(VfsError::Unsupported),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_pollable(&self) -> Option<&dyn Pollable> {
        Some(self)
    }

    fn flags(&self) -> NodeFlags {
        NodeFlags::NON_CACHEABLE
    }

    fn mmap(&self, _offset: u64, _length: u64) -> DeviceMmap {
        // 设备层会在这个区间上按 mmap 的 offset 调整，并强制 UNCACHED 映射，
        // 因此用户态看到的就是与小核共享的那块非缓存内存。
        match self.shared.as_ref() {
            Some(shared) => DeviceMmap::Physical(
                PhysAddrRange::from_start_size(
                    PhysAddr::from(shared.paddr),
                    shared.size,
                ),
                None,
            ),
            None => DeviceMmap::None,
        }
    }
}

impl Pollable for CmdquDevice {
    fn poll(&self) -> IoEvents {
        let mut events = IoEvents::empty();
        events.set(IoEvents::IN, !self.rx_is_empty());
        events
    }

    unsafe fn register_shared(&self, sink: &mut dyn SharedRegistrationSink, events: IoEvents) {
        if !events.contains(IoEvents::IN) {
            return;
        }
        POLL_REGISTRATIONS.fetch_add(1, Ordering::AcqRel);
        unsafe { sink.register_shared(&POLL_WAITERS, IoEvents::IN) };
        // 注册与检查之间可能已经有消息到达，这里补一次唤醒避免丢事件。
        if !self.rx_is_empty() {
            unsafe { POLL_WAITERS.wake(IoEvents::IN) };
        }
    }
}
