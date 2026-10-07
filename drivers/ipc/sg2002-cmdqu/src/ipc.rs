//! 把本 crate 的 mailbox/槽位实现成 [`rdif_ipc`] 的跨核消息通道后端。
//!
//! 分工：本文件是**硬件层**——发信、把硬件里已到达的信封搬进定长队列、报告对端是否在跑；
//! "等一条指定应答"（超时、睡眠、唤醒）留在操作系统粘合层，因为它依赖调度器，
//! 而本 crate 不能依赖任何 OS。

use rdif_ipc::{DriverGeneric, IpcError, IpcMessage, RemoteState, SharedWindow};

use crate::{mailbox::Mailbox, protocol::Envelope, queue::EnvelopeQueue};

/// 收信队列深度。8 个 mailbox 槽位是"硬件在途"，这里放的是已取回、等消费者取走的；
/// 64 条足够吸收一次控制周期里的突发，又不需要分配内存（可在中断上下文 push）。
pub const RX_QUEUE_DEPTH: usize = 64;

/// `IpcMessage::flags` 的位 0：等价于信封里的 `block`（要求对端回信）。
pub const FLAG_BLOCK: u8 = 1 << 0;

/// SG2002 大小核命令队列的 IPC 后端。
pub struct Sg2002Ipc {
    mailbox: Mailbox,
    rx: EnvelopeQueue<RX_QUEUE_DEPTH>,
    sent: u64,
    received: u64,
    /// 设备树声明了共享区才有；由操作系统粘合层探测后通过 [`Self::with_shared_window`] 注入。
    shared: Option<SharedWindow>,
}

impl Sg2002Ipc {
    pub const fn new(mailbox: Mailbox) -> Self {
        Self {
            mailbox,
            rx: EnvelopeQueue::new(),
            sent: 0,
            received: 0,
            shared: None,
        }
    }

    /// 登记共享内存窗口（物理地址与长度）。布局由调用方与对端约定，本层不解释。
    pub fn with_shared_window(mut self, window: SharedWindow) -> Self {
        self.shared = Some(window);
        self
    }

    /// 直接访问 mailbox 寄存器：操作系统粘合层用它做中断注册、槽位转储、PC 采样等
    /// 不属于通用 IPC 能力的动作。
    pub const fn mailbox(&self) -> &Mailbox {
        &self.mailbox
    }

    /// 已成功投递的条数。
    pub const fn sent(&self) -> u64 {
        self.sent
    }

    /// 已从硬件取回的条数（含还在队列里没被取走的）。
    pub const fn received(&self) -> u64 {
        self.received
    }

    /// 因队列满而被丢掉的条数。
    pub const fn dropped(&self) -> u64 {
        self.rx.dropped()
    }

    /// 取出第一条 `(ip_id, cmd_id)` 匹配的应答，其余保持顺序。
    ///
    /// 操作系统粘合层用它实现"发送并等待**指定**应答"——这是厂商驱动做不到的那件事。
    pub fn take_matching(&mut self, ip_id: u8, cmd_id: u8) -> Option<IpcMessage> {
        self.rx.remove_matching(ip_id, cmd_id).map(IpcMessage::from)
    }

    pub fn has_matching(&self, ip_id: u8, cmd_id: u8) -> bool {
        self.rx.contains_matching(ip_id, cmd_id)
    }

    /// 投递一个协议信封（不经 [`IpcMessage`]）。
    ///
    /// 给需要**与厂商 ABI 逐位一致**的调用方用：用户态可以自造头部字，方向位不一定是
    /// "大核发出"那一组；`send()` 那条通用路径会按方向重新编码，这里不重编码。
    pub fn send_envelope(&mut self, envelope: Envelope) -> Result<usize, IpcError> {
        match self.mailbox.send(envelope) {
            Ok(slot) => {
                self.sent += 1;
                Ok(slot)
            }
            Err(_) => Err(IpcError::WouldBlock),
        }
    }

    /// 取一条原始信封（保留协议头部的全部位），给用户态 ABI 用。
    pub fn pop_envelope(&mut self) -> Option<Envelope> {
        self.rx.pop()
    }

    /// 取一条匹配的原始信封，其余保持 FIFO 顺序。
    pub fn take_matching_envelope(&mut self, ip_id: u8, cmd_id: u8) -> Option<Envelope> {
        self.rx.remove_matching(ip_id, cmd_id)
    }

    /// 硬件里是否还有待搬的信封（软件队列为空时也有可能是 true）。
    pub fn hardware_pending(&self) -> bool {
        self.mailbox.pending_mask() != 0
    }
}

impl DriverGeneric for Sg2002Ipc {
    fn name(&self) -> &str {
        "sg2002-cmdqu"
    }
}

impl From<Envelope> for IpcMessage {
    fn from(envelope: Envelope) -> Self {
        Self {
            ip_id: envelope.ip_id,
            cmd_id: envelope.cmd_id,
            flags: if envelope.block { FLAG_BLOCK } else { 0 },
            param: envelope.param_ptr,
        }
    }
}

impl rdif_ipc::Interface for Sg2002Ipc {
    fn send(&mut self, msg: IpcMessage) -> Result<(), IpcError> {
        let envelope = Envelope::request_raw(
            msg.ip_id,
            msg.cmd_id,
            msg.flags & FLAG_BLOCK != 0,
            msg.param,
        );
        self.send_envelope(envelope).map(|_| ())
    }

    fn pump(&mut self) {
        while let Some((_slot, envelope)) = self.mailbox.take_pending() {
            self.rx.push(envelope);
            self.received += 1;
        }
    }

    fn try_recv(&mut self) -> Option<IpcMessage> {
        self.pop_envelope().map(IpcMessage::from)
    }

    fn is_ready(&self) -> bool {
        !self.rx.is_empty()
    }

    fn remote_state(&self) -> RemoteState {
        // PC 监视寄存器为 0 说明小核还在复位态（实板判据，见 docs/11 第 8.4 节）。
        if self.mailbox.pc_monitor() == 0 {
            RemoteState::Halted
        } else {
            RemoteState::Running
        }
    }

    fn shared_window(&self) -> Option<SharedWindow> {
        self.shared
    }
}
