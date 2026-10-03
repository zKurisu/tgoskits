//! 跨核消息通道的驱动接口。
//!
//! 抽象的是"与另一颗处理器交换定长消息，并可能共享一块内存"这一能力，
//! 不关心对端是谁、用什么传输（mailbox、共享内存环、虚拟化通道都可以）。
//! 具体后端只需要回答三件事：**怎么发**（`send`）、**怎么把硬件队列里的消息搬进来**
//! （`pump` + `try_recv`）、以及**对端是不是还活着 / 有没有共享窗口**（后两者有默认实现）。
//!
//! 设计上刻意把"等一条指定应答"（超时、唤醒、队列）留在操作系统粘合层：
//! 那是调度与睡眠语义，不是硬件能力——后端在中断或轮询上下文里不该阻塞。
//! 内核侧的典型用法是：`pump()` 把消息搬进队列 → 唤醒等待者 → 等待者用
//! `try_recv()` 匹配它要的那条（见 StarryOS 的 `pseudofs/dev/cmdqu.rs`）。

#![no_std]

extern crate alloc;

use rdif_base::def_driver;
pub use rdif_base::{DriverGeneric, KError};

/// 一条跨核消息。
///
/// `ip_id` 标识对端子系统、`cmd_id` 是命令号、`param` 是随命令的一个 32 位参数
/// （通常是指向共享区的小结构体偏移，或直接是一个标量）。`flags` 留给具体协议
/// 表达"需要回信""只等待不发送"之类的位，接口本身不解释它。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IpcMessage {
    pub ip_id: u8,
    pub cmd_id: u8,
    pub flags: u8,
    pub param: u32,
}

impl IpcMessage {
    pub const fn new(ip_id: u8, cmd_id: u8, param: u32) -> Self {
        Self {
            ip_id,
            cmd_id,
            flags: 0,
            param,
        }
    }
}

/// 对端处理器的运行状态。后端不一定能观测到（例如纯共享内存通道），
/// 观测不到时返回 [`RemoteState::Unknown`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RemoteState {
    #[default]
    Unknown,
    Running,
    Halted,
}

/// 两端都能访问的内存窗口。
///
/// 接口只承诺"这块内存在两边都可见"，**里面的布局由具体协议约定**：
/// 消息通道本身不需要它，需要传大块数据或做状态上报的协议才会用到。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedWindow {
    /// 物理地址（内核侧自行决定怎么做非缓存映射）。
    pub paddr: u64,
    /// 长度（字节）。
    pub size: usize,
}

#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcError {
    #[error("remote core did not accept the message")]
    Hardware,
    #[error("no free slot in the outbound queue")]
    WouldBlock,
    #[error("operation not supported by this backend")]
    Unsupported,
}

pub trait Interface: DriverGeneric {
    /// 投递一条消息（不等待对端应答）。
    fn send(&mut self, msg: IpcMessage) -> Result<(), IpcError>;

    /// 把硬件里已经到达的消息搬进后端内部队列。
    ///
    /// 约定是**幂等**且可在中断上下文调用：没消息时什么都不做，有消息时全部搬走。
    /// 内核在中断处理里调用它，然后唤醒等待者。
    fn pump(&mut self);

    /// 取一条已经到达的消息；队列为空时返回 `None`（不阻塞）。
    fn try_recv(&mut self) -> Option<IpcMessage>;

    /// 队列里是否已有消息——`poll`/`read` 的就绪判断用它。
    fn is_ready(&self) -> bool;

    /// 对端处理器的运行状态；观测不到的后端保持默认实现。
    fn remote_state(&self) -> RemoteState {
        RemoteState::Unknown
    }

    /// 与对端共享的内存窗口；没有共享内存的后端保持默认实现。
    fn shared_window(&self) -> Option<SharedWindow> {
        None
    }
}

def_driver!(Ipc, Interface);
