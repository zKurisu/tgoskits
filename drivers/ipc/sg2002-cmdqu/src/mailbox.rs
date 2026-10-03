//! mailbox 寄存器与 8 个槽位的操作。
//!
//! 调用方负责先把寄存器窗口映射成**非缓存**虚拟地址，再传给 [`Mailbox::new`]。
//! 所有偏移都与厂商头文件 `cvi_mailbox.h` 一致：本核（大核 C906B）是
//! `RECEIVE_CPU = 1`，对端小核是 `SEND_TO_CPU = 2`。

use crate::protocol::{Envelope, SLOT_COUNT};

/// 寄存器窗口长度（设备树里 `rtos_cmdqu` 声明的就是 4 KiB）。
pub const MAILBOX_WINDOW: usize = 0x1000;
/// 消息缓冲区相对窗口基址的偏移。
pub const CONTEXT_OFFSET: usize = 0x0400;
/// 小核程序计数器监视寄存器相对窗口基址的偏移（越过 4 KiB，需要额外映射）。
pub const PC_MONITOR_OFFSET: usize = 0x1070;

/// 本核（CPU1）的接收使能位。
const REG_EN_OWN: usize = 0x04;
/// 对端（CPU2）的接收使能位，由发送方置位。
const REG_EN_PEER: usize = 0x08;
/// 本核中断清除寄存器（写 1 清除对应位）。
const REG_CLR_OWN: usize = 0x20;
/// 本核中断状态寄存器。
const REG_INT_OWN: usize = 0x28;
/// 对端中断清除寄存器。
const REG_CLR_PEER: usize = 0x30;
/// 门铃寄存器。
const REG_DOORBELL: usize = 0x60;

/// 发送失败的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailboxError {
    /// 8 个槽位都被占用。
    NoFreeSlot,
}

/// mailbox 硬件句柄。
#[derive(Debug)]
pub struct Mailbox {
    base: usize,
}

impl Mailbox {
    /// 用已映射的寄存器窗口基址创建句柄。
    ///
    /// # Safety
    ///
    /// 调用方必须保证 `base` 指向一段可读写的非缓存 MMIO 映射，且在句柄存续
    /// 期间保持有效。
    pub const unsafe fn new(base: usize) -> Self {
        Self { base }
    }

    /// 已映射的窗口基址。
    pub const fn base(&self) -> usize {
        self.base
    }

    fn rd8(&self, offset: usize) -> u8 {
        unsafe { core::ptr::read_volatile((self.base + offset) as *const u8) }
    }

    fn wr8(&self, offset: usize, value: u8) {
        unsafe { core::ptr::write_volatile((self.base + offset) as *mut u8, value) }
    }

    fn rd32(&self, offset: usize) -> u32 {
        unsafe { core::ptr::read_volatile((self.base + offset) as *const u32) }
    }

    fn wr32(&self, offset: usize, value: u32) {
        unsafe { core::ptr::write_volatile((self.base + offset) as *mut u32, value) }
    }

    /// 本核待处理槽位的位图。
    pub fn pending_mask(&self) -> u8 {
        self.rd8(REG_INT_OWN)
    }

    /// 发送方已经置过的对端使能位（正常情况下发送后会由对端清除）。
    pub fn peer_enable_mask(&self) -> u8 {
        self.rd8(REG_EN_PEER)
    }

    /// 本核接收使能位（收到消息并应答后由本核清除）。
    pub fn own_enable_mask(&self) -> u8 {
        self.rd8(REG_EN_OWN)
    }

    /// 读某个槽位的两个 32 位字。
    pub fn slot_words(&self, slot: usize) -> (u32, u32) {
        let offset = CONTEXT_OFFSET + slot * 8;
        (self.rd32(offset), self.rd32(offset + 4))
    }

    /// 把某个槽位清零（归还给双方）。
    pub fn clear_slot(&self, slot: usize) {
        let offset = CONTEXT_OFFSET + slot * 8;
        self.wr32(offset, 0);
        self.wr32(offset + 4, 0);
    }

    /// 读小核程序计数器监视寄存器。
    pub fn pc_monitor(&self) -> u32 {
        self.rd32(PC_MONITOR_OFFSET)
    }

    /// 把一个信封写进空槽并拉门铃，返回所用槽位号。
    ///
    /// 序列与厂商驱动逐字一致：清对端中断位 → 置对端使能位 → 写门铃。
    pub fn try_send(&self, envelope: Envelope) -> Result<usize, MailboxError> {
        for slot in 0..SLOT_COUNT {
            let (header, param) = self.slot_words(slot);
            if !Envelope::is_empty_slot(header, param) {
                continue;
            }
            let (word0, word1) = envelope.words();
            self.wr32(CONTEXT_OFFSET + slot * 8, word0);
            self.wr32(CONTEXT_OFFSET + slot * 8 + 4, word1);
            self.wr8(REG_CLR_PEER, 1u8 << slot);
            let enabled = self.rd8(REG_EN_PEER);
            self.wr8(REG_EN_PEER, enabled | (1u8 << slot));
            self.wr8(REG_DOORBELL, 1u8 << slot);
            return Ok(slot);
        }
        Err(MailboxError::NoFreeSlot)
    }

    /// 取走一个待处理消息：读内容、归还槽位、清中断位与使能位。
    ///
    /// 返回 `(槽位号, 信封)`；没有待处理消息时返回 `None`。
    pub fn take_pending(&self) -> Option<(usize, Envelope)> {
        let mask = self.pending_mask();
        if mask == 0 {
            return None;
        }
        let slot = mask.trailing_zeros() as usize;
        if slot >= SLOT_COUNT {
            // 状态位里出现了 8 个槽位之外的位，属于硬件异常：清掉并放弃。
            self.wr8(REG_CLR_OWN, mask);
            return None;
        }
        let (header, param) = self.slot_words(slot);
        self.clear_slot(slot);
        self.wr8(REG_CLR_OWN, 1u8 << slot);
        let enabled = self.rd8(REG_EN_OWN);
        self.wr8(REG_EN_OWN, enabled & !(1u8 << slot));
        Some((slot, Envelope::from_words(header, param)))
    }

    /// 发送一条即发即弃的消息。
    pub fn send(&self, envelope: Envelope) -> Result<usize, MailboxError> {
        self.try_send(envelope)
    }

    /// 门铃之后等待应答时用：对端把回复写进空槽会置起本核中断位，
    /// 直接轮询状态位即可（不需要中断也能工作）。
    pub fn poll_reply(&self) -> Option<Envelope> {
        self.take_pending().map(|(_, envelope)| envelope)
    }
}
