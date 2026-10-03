//! SG2002（Cvitek）大小核命令队列（command queue，简称 cmdqu）硬件层。
//!
//! 本 crate 只包含与操作系统无关的部分：8 字节信封的编解码、mailbox 寄存器
//! 与 8 个槽位的读写、以及门铃/应答序列。文件描述符、ioctl、等待队列等 OS
//! 粘合代码由使用它的内核提供（StarryOS 侧见
//! `os/StarryOS/kernel/src/pseudofs/dev/cmdqu.rs`）。
//!
//! 调用方必须先把这个寄存器窗口映射成**非缓存**虚拟地址再传进来：该窗口被
//! 两个核共享，按可缓存映射会让写停留在缓存里，表现为"消息没人取"。

#![cfg_attr(not(test), no_std)]

pub mod mailbox;
pub mod protocol;
pub mod shm;

pub use mailbox::{Mailbox, MailboxError};
pub use protocol::{EncodeError, Envelope, IpId, SysCmdId};
