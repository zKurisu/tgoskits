//! 收到的信封的定长 FIFO 队列。
//!
//! 放在硬件层而不是操作系统粘合层，因为它描述的是"硬件可能随时把消息塞进来、
//! 而消费者可能一时跟不上"这件事：队列**不分配内存**（可以在中断上下文里 push），
//! 满了就丢最旧的一条并计数——丢的是最旧而不是最新，是为了让"最近发生了什么"
//! 留在队列里，便于事后诊断。

use crate::protocol::Envelope;

#[derive(Debug)]
pub struct EnvelopeQueue<const N: usize> {
    slots: [Option<Envelope>; N],
    head: usize,
    len: usize,
    dropped: u64,
}

impl<const N: usize> EnvelopeQueue<N> {
    pub const fn new() -> Self {
        Self {
            slots: [None; N],
            head: 0,
            len: 0,
            dropped: 0,
        }
    }

    /// 入队；队列满时丢最旧的一条并让 `dropped` 加一。
    pub fn push(&mut self, envelope: Envelope) {
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

    pub fn pop(&mut self) -> Option<Envelope> {
        if self.len == 0 {
            return None;
        }
        let envelope = self.slots[self.head].take();
        self.head = (self.head + 1) % N;
        self.len -= 1;
        envelope
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    /// 因队列满而丢掉的条数。
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }

    /// 取出第一条 `(ip_id, cmd_id)` 匹配的消息，其余保持 FIFO 顺序。
    ///
    /// 操作系统粘合层用它匹配"等的是哪一条应答"，而不必先出队再塞回去。
    pub fn remove_matching(&mut self, ip_id: u8, cmd_id: u8) -> Option<Envelope> {
        for offset in 0..self.len {
            let index = (self.head + offset) % N;
            let Some(envelope) = self.slots[index] else {
                continue;
            };
            if envelope.ip_id != ip_id || envelope.cmd_id != cmd_id {
                continue;
            }
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
    pub fn contains_matching(&self, ip_id: u8, cmd_id: u8) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn env(cmd_id: u8) -> Envelope {
        Envelope::request_raw(6, cmd_id, false, cmd_id as u32)
    }

    #[test]
    fn push_pop_preserves_order() {
        let mut queue = EnvelopeQueue::<3>::new();
        queue.push(env(1));
        queue.push(env(2));
        queue.push(env(3));
        assert_eq!(queue.len(), 3);
        assert_eq!(queue.pop().unwrap().cmd_id, 1);
        assert_eq!(queue.pop().unwrap().cmd_id, 2);
        queue.push(env(4));
        assert_eq!(queue.pop().unwrap().cmd_id, 3);
        assert_eq!(queue.pop().unwrap().cmd_id, 4);
        assert!(queue.pop().is_none());
        assert_eq!(queue.dropped(), 0);
    }

    #[test]
    fn overflow_drops_the_oldest_and_counts_it() {
        let mut queue = EnvelopeQueue::<2>::new();
        queue.push(env(1));
        queue.push(env(2));
        queue.push(env(3));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.dropped(), 1);
        // 丢的是最旧的一条，最近的消息留在队列里
        assert_eq!(queue.pop().unwrap().cmd_id, 2);
        assert_eq!(queue.pop().unwrap().cmd_id, 3);
    }

    #[test]
    fn remove_matching_takes_the_matching_one_and_keeps_the_rest_in_order() {
        let mut queue = EnvelopeQueue::<4>::new();
        queue.push(env(1));
        queue.push(Envelope::request_raw(6, 2, false, 0));
        queue.push(env(3));

        let taken = queue.remove_matching(6, 2).unwrap();
        assert_eq!(taken.cmd_id, 2);
        assert_eq!(queue.len(), 2);
        assert!(!queue.contains_matching(6, 2));
        assert_eq!(queue.pop().unwrap().cmd_id, 1);
        assert_eq!(queue.pop().unwrap().cmd_id, 3);
        assert!(queue.remove_matching(6, 9).is_none());
    }
}
