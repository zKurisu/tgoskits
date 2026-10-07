//! 跨核共享缓冲区的布局与消息格式。
//!
//! 设计目标有三个：**自描述**（头部带魔数与版本，协议演进不必重新划分地址）、
//! **可校验**（每条消息带长度与 CRC32）、**无伪共享**（见下）。
//!
//! **无伪共享在这一版升级为"按写入方独占 cache line"。** 原因来自 2026-10-02 的
//! 实测：小核用 `flush_dcache_range(&hdr->rtos_to_host, 20)` 只回写 20 字节，但
//! C906 的回写粒度是**整条 64 字节 cache line**，于是它把同一行里大核拥有的
//! `b_tail`（消费者索引）连带用旧副本覆盖回 DRAM，大核按 tail 记账时条目数忽多忽少
//! （一度被误读成"小核 tick 慢 2.19%")。因此头部拆成三条 line，每条只有一个写入方：
//!
//! ```text
//! 行 0（0x0000..0x003F）常量区：大核初始化时写一次，之后双方只读
//! 行 1（0x0040..0x007F）大核写、小核读：命令环 head/tail、状态环 tail
//! 行 2（0x0080..0x00BF）小核写、大核读：状态环 head
//! 行 3（0x00C0..0x00FF）时间同步：大核写的一半
//! 行 4（0x0100..0x013F）时间同步：小核写的一半
//! ```
//!
//! ```text
//! 偏移 0x0000  ShmHeader（192 字节 = 3 条 cache line，按写入方分行）
//! 偏移 0x00C0  SyncHostBlock + SyncRtosBlock（各 64 字节，按写入方分行）
//! 偏移 0x0140  Ring A：大核 → 小核（64 项 × 64 字节）
//! 偏移 0x1140  Ring B：小核 → 大核（64 项 × 64 字节）
//! 其余空间    保留给批量数据（图像、大块日志），当前未使用
//! ```
//!
//! **为什么是两个环**：单环只能有一个生产者。状态快照是小核→大核，命令是大核→
//! 小核，方向相反；若共用一个环，"谁写 head"就没有一致定义。拆成两个环之后，
//! 每个环都是"单生产者单消费者"，`head` 只由生产者写、`tail` 只由消费者写，
//! 因此不需要跨核锁。
//!
//! 访问约定：本侧（大核）把整块区域映射为**非缓存**，小核侧也必须以非缓存方式
//! 访问（C906 的 D-cache 不参与跨核一致性）。环形缓冲的 `head` 只由大核写、
//! `tail` 只由小核写，单写者单读者，因此不需要跨核锁。

/// 头部魔数，ASCII "CQSH"。
pub const SHM_MAGIC: u32 = 0x4351_5348;
/// 当前布局版本。
///
/// **任何布局改动都必须递增此值**：共享区位于 DRAM，内容会跨复位保留，
/// 版本不变就可能被新内核当成"兼容的旧头部"而复用，字段含义却是错的。
///
/// 版本历史：
/// - 2：单行头部（64 字节），两个环描述各含 head/tail；
/// - 3：头部按写入方拆成三条 cache line，head/tail 提为独立字段，同步块拆成
///   大核写与小核写两行——修掉"整行回写覆盖对端字段"的缺陷。
pub const SHM_VERSION: u32 = 3;
/// cache line 大小；头部按此对齐，避免与环缓冲伪共享。
pub const CACHE_LINE: usize = 64;
/// 头部固定长度：三条 cache line（常量区 / 大核可写区 / 小核可写区）。
pub const HEADER_SIZE: usize = 3 * CACHE_LINE;
/// 时间基准同步块：两条 cache line（大核一行、小核一行）。
pub const SYNC_HOST_OFFSET: usize = HEADER_SIZE;
pub const SYNC_RTOS_OFFSET: usize = SYNC_HOST_OFFSET + CACHE_LINE;
pub const SYNC_SIZE: usize = 2 * CACHE_LINE;
/// 兼容别名：同步块整体起始偏移（等于大核写的那一行）。
pub const SYNC_OFFSET: usize = SYNC_HOST_OFFSET;
/// 两个环的起始偏移。
pub const RING_HOST_TO_RTOS_OFFSET: usize = SYNC_OFFSET + SYNC_SIZE;
pub const RING_RTOS_TO_HOST_OFFSET: usize = RING_HOST_TO_RTOS_OFFSET + RING_BYTES;
/// 每条环消息的固定长度。
pub const RING_ENTRY_SIZE: usize = 64;
/// 环消息的载荷长度（扣掉 16 字节元信息）。
pub const RING_PAYLOAD_SIZE: usize = RING_ENTRY_SIZE - 16;
/// 每个环的条目数。
pub const RING_ENTRIES: u32 = 64;
/// 单个环的字节长度。
pub const RING_BYTES: usize = RING_ENTRIES as usize * RING_ENTRY_SIZE;

/// 消息类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum MessageKind {
    /// 周期性状态快照（小核 → 大核）。
    StateSnapshot = 1,
    /// 事件上报：碰撞、堵转、急停（小核 → 大核）。
    Event         = 2,
    /// 心跳（双向）。
    Heartbeat     = 3,
    /// 主机下发的配置或目标值（大核 → 小核）。
    HostCommand   = 4,
    /// 时间基准同步（双向，配合 mailbox 里的 `SYS_CMD_SYNC_TIME`）。
    TimeSync      = 5,
}

impl MessageKind {
    /// 从原始数值还原；未知类型返回 `None`，调用方应丢弃该条目。
    pub const fn from_raw(raw: u32) -> Option<Self> {
        Some(match raw {
            1 => Self::StateSnapshot,
            2 => Self::Event,
            3 => Self::Heartbeat,
            4 => Self::HostCommand,
            5 => Self::TimeSync,
            _ => return None,
        })
    }
}

/// 一个环的**常量部分**：起始偏移、条目数、条目长度。
///
/// 可变索引（`head`/`tail`）刻意不放在这里：它们必须落在"按写入方独占"的
/// cache line 上，否则对端的整行回写会把它们覆盖掉（见文件头说明）。
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingLayout {
    /// 环在共享区内的起始偏移。
    pub offset: u32,
    /// 条目数。
    pub entries: u32,
    /// 每条消息长度。
    pub entry_size: u32,
}

impl RingLayout {
    /// 构造一个空环描述。
    pub const fn new(offset: usize) -> Self {
        Self {
            offset: offset as u32,
            entries: RING_ENTRIES,
            entry_size: RING_ENTRY_SIZE as u32,
        }
    }

    /// 环的字节长度。
    pub const fn bytes(&self) -> u32 {
        self.entries * self.entry_size
    }

    /// 描述是否落在共享区内且长度自洽。
    pub const fn is_valid(&self, total: u32) -> bool {
        self.entries > 0
            && self.entry_size as usize == RING_ENTRY_SIZE
            && self.offset as usize >= RING_HOST_TO_RTOS_OFFSET
            && (self.offset as usize).is_multiple_of(CACHE_LINE)
            && (self.offset as usize) + self.bytes() as usize <= total as usize
    }
}

/// 共享区头部，固定 192 字节（三条 cache line），按"谁写"分行。
///
/// ```text
/// 行 0（0..63）  常量：大核初始化时写一次，之后双方只读
/// 行 1（64..127）大核独占写：命令环 head/tail、状态环 tail（消费者索引）
/// 行 2（128..191）小核独占写：状态环 head（生产者索引）
/// ```
///
/// 行 1 与行 2 必须分开：小核发布状态快照时会回写自己那一行，若大核的
/// `rtos_to_host_tail` 与之同行，就会被旧副本覆盖。
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShmHeader {
    // ── 行 0：常量区（只读） ──
    /// 必须等于 [`SHM_MAGIC`]。
    pub magic: u32,
    /// 布局版本，必须等于 [`SHM_VERSION`]。
    pub version: u32,
    /// 共享区总大小（字节）。
    pub size: u32,
    /// 头部长度，用于前向兼容。
    pub header_size: u32,
    /// 大核 → 小核的环（命令环）的常量部分。
    pub host_to_rtos: RingLayout,
    /// 小核 → 大核的环（状态环）的常量部分。
    pub rtos_to_host: RingLayout,
    /// 补齐到 64 字节。
    pub reserved0: [u8; 24],

    // ── 行 1：大核写、小核读 ──
    /// 命令环生产者索引（大核写）。
    pub host_to_rtos_head: u32,
    /// 命令环回收索引（大核写；当前固件只读）。
    pub host_to_rtos_tail: u32,
    /// 状态环消费者索引（大核写）。
    pub rtos_to_host_tail: u32,
    /// 补齐到 64 字节。
    pub reserved1: [u8; 52],

    // ── 行 2：小核写、大核读 ──
    /// 状态环生产者索引（小核写）。
    pub rtos_to_host_head: u32,
    /// 补齐到 64 字节。
    pub reserved2: [u8; 60],
}

impl ShmHeader {
    /// 生成一个头部（两个环都为空）。
    pub const fn new(size: usize) -> Self {
        Self {
            magic: SHM_MAGIC,
            version: SHM_VERSION,
            size: size as u32,
            header_size: HEADER_SIZE as u32,
            host_to_rtos: RingLayout::new(RING_HOST_TO_RTOS_OFFSET),
            rtos_to_host: RingLayout::new(RING_RTOS_TO_HOST_OFFSET),
            reserved0: [0; 24],
            host_to_rtos_head: 0,
            host_to_rtos_tail: 0,
            rtos_to_host_tail: 0,
            reserved1: [0; 52],
            rtos_to_host_head: 0,
            reserved2: [0; 60],
        }
    }

    /// 命令环里尚未被小核取走的条目数。
    pub const fn host_to_rtos_pending(&self) -> u32 {
        self.host_to_rtos_head.wrapping_sub(self.host_to_rtos_tail)
    }

    /// 状态环里尚未被大核消费的条目数。
    pub const fn rtos_to_host_pending(&self) -> u32 {
        self.rtos_to_host_head.wrapping_sub(self.rtos_to_host_tail)
    }

    /// 头部是否可识别且版本匹配。
    pub const fn is_compatible(&self) -> bool {
        self.magic == SHM_MAGIC
            && self.version == SHM_VERSION
            && self.header_size as usize == HEADER_SIZE
            && self.host_to_rtos.is_valid(self.size)
            && self.rtos_to_host.is_valid(self.size)
    }
}

/// 时间基准同步块的**大核半边**，固定 64 字节（独占一条 cache line）。
///
/// 同步流程：大核把自己的时刻写进 `host_time_us` 并递增 `seq`，小核读到后
/// 把对应时刻写进自己的 [`SyncRtosBlock`]，大核据此估算偏移与频率比。
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncHostBlock {
    /// 同步序号，每次发起同步递增。
    pub seq: u32,
    /// 标志位（保留）。
    pub flags: u32,
    /// 大核在本次同步中记录的时刻（微秒）。
    pub host_time_us: u64,
    /// 大核记录的时刻对应的状态环序号，供小核配对。
    pub rtos_to_host_head: u32,
    /// 补齐到 64 字节。
    pub reserved: [u8; 44],
}

impl SyncHostBlock {
    /// 全零的同步块。
    pub const fn new() -> Self {
        Self {
            seq: 0,
            flags: 0,
            host_time_us: 0,
            rtos_to_host_head: 0,
            reserved: [0; 44],
        }
    }
}

impl Default for SyncHostBlock {
    fn default() -> Self {
        Self::new()
    }
}

/// 时间基准同步块的**小核半边**，固定 64 字节（独占一条 cache line）。
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncRtosBlock {
    /// 小核回填时对应的同步序号；大核用它丢弃过期回填。
    pub seq: u32,
    /// 标志位（保留）。
    pub flags: u32,
    /// 小核回填的时刻（微秒，基于小核 tick 计数换算）。
    pub rtos_time_us: u64,
    /// 补齐到 64 字节。
    pub reserved: [u8; 48],
}

impl SyncRtosBlock {
    /// 全零的同步块。
    pub const fn new() -> Self {
        Self {
            seq: 0,
            flags: 0,
            rtos_time_us: 0,
            reserved: [0; 48],
        }
    }
}

impl Default for SyncRtosBlock {
    fn default() -> Self {
        Self::new()
    }
}

/// 一条环消息：16 字节元信息 + 48 字节载荷。
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingEntry {
    /// 消息类型（见 [`MessageKind`]）。
    pub kind: u32,
    /// 生产者序号，与 `ring_head` 对应，用于检测丢条目。
    pub seq: u32,
    /// 载荷有效字节数。
    pub len: u32,
    /// 载荷的 CRC32（对 `payload[..len]` 计算）。
    pub crc: u32,
    /// 载荷。
    pub payload: [u8; RING_PAYLOAD_SIZE],
}

impl RingEntry {
    /// 用载荷构造一条消息；载荷过长返回 `None`。
    pub fn encode(kind: MessageKind, seq: u32, payload: &[u8]) -> Option<Self> {
        if payload.len() > RING_PAYLOAD_SIZE {
            return None;
        }
        let mut entry = Self {
            kind: kind as u32,
            seq,
            len: payload.len() as u32,
            crc: 0,
            payload: [0; RING_PAYLOAD_SIZE],
        };
        entry.payload[..payload.len()].copy_from_slice(payload);
        entry.crc = crc32(payload);
        Some(entry)
    }

    /// 校验并取出载荷；类型未知、长度越界或 CRC 不符都返回 `None`。
    pub fn decode(&self) -> Option<(MessageKind, u32, &[u8])> {
        let kind = MessageKind::from_raw(self.kind)?;
        let len = self.len as usize;
        if len > RING_PAYLOAD_SIZE {
            return None;
        }
        let payload = &self.payload[..len];
        if crc32(payload) != self.crc {
            return None;
        }
        Some((kind, self.seq, payload))
    }
}

/// 状态快照载荷：拾球场景里"小核 → 大核"周期上报的固定结构。
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct StateSnapshot {
    /// 采样序号。
    pub seq: u32,
    /// 标志位（如急停、堵转）。
    pub flags: u32,
    /// 小核侧的采样时刻（微秒）。
    pub rtos_time_us: u64,
    /// 两路轮速计数（增量或累计，由固件约定）。
    pub wheel_counts: [i32; 2],
    /// 光电门位图。
    pub photogate: u32,
    /// 两路 PWM 占空比（千分比）。
    pub duty_permille: [u16; 2],
}

/// 状态快照里的标志位。
pub mod snapshot_flags {
    /// 急停生效。
    pub const ESTOP: u32 = 1 << 0;
    /// 堵转检测。
    pub const STALL: u32 = 1 << 1;
    /// 与大核的通信超时过。
    pub const HOST_TIMEOUT: u32 = 1 << 2;
}

/// CRC32（IEEE 802.3 多项式 0xEDB88320），逐位实现，避免为小区域引入查表。
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_three_cache_lines() {
        assert_eq!(core::mem::size_of::<ShmHeader>(), HEADER_SIZE);
        assert_eq!(HEADER_SIZE, 3 * CACHE_LINE);
    }

    /// 这条测试守的是 2026-10-02 实板暴露的缺陷：小核回写自己那一行时，
    /// 会把同一行里大核拥有的索引一起用旧副本覆盖。因此"大核写的字段"与
    /// "小核写的字段"必须落在不同 cache line 上。
    #[test]
    fn host_written_and_rtos_written_fields_never_share_a_cache_line() {
        let host_line = core::mem::offset_of!(ShmHeader, rtos_to_host_tail) / CACHE_LINE;
        let host_line_head = core::mem::offset_of!(ShmHeader, host_to_rtos_head) / CACHE_LINE;
        let host_line_cmd_tail = core::mem::offset_of!(ShmHeader, host_to_rtos_tail) / CACHE_LINE;
        let rtos_line = core::mem::offset_of!(ShmHeader, rtos_to_host_head) / CACHE_LINE;
        assert_eq!(host_line, host_line_head);
        assert_eq!(host_line, host_line_cmd_tail);
        assert_ne!(host_line, rtos_line);

        // 常量区也必须独占一行：小核只读它，大核只在初始化时写一次。
        let const_line = core::mem::offset_of!(ShmHeader, magic) / CACHE_LINE;
        assert_eq!(const_line, 0);
        assert_eq!(host_line, 1);
        assert_eq!(rtos_line, 2);

        // 同步块同样按写入方分两行。
        assert_eq!(SYNC_RTOS_OFFSET - SYNC_HOST_OFFSET, CACHE_LINE);
        assert_eq!(core::mem::size_of::<SyncHostBlock>(), CACHE_LINE);
        assert_eq!(core::mem::size_of::<SyncRtosBlock>(), CACHE_LINE);
        assert_eq!(SYNC_HOST_OFFSET % CACHE_LINE, 0);
        assert_eq!(SYNC_RTOS_OFFSET % CACHE_LINE, 0);
        assert_eq!(RING_HOST_TO_RTOS_OFFSET % CACHE_LINE, 0);
        assert_eq!(RING_RTOS_TO_HOST_OFFSET % CACHE_LINE, 0);
    }

    #[test]
    fn entry_layout_is_16_plus_payload() {
        assert_eq!(core::mem::size_of::<RingEntry>(), RING_ENTRY_SIZE);
        assert_eq!(RING_PAYLOAD_SIZE, 48);
    }

    #[test]
    fn header_round_trip_and_validation() {
        let mut header = ShmHeader::new(1024 * 1024);
        assert!(header.is_compatible());
        assert_eq!(header.rtos_to_host_pending(), 0);

        // 两个环互不重叠，且都落在共享区内
        assert_eq!(
            header.host_to_rtos.offset as usize,
            RING_HOST_TO_RTOS_OFFSET
        );
        assert_eq!(
            header.rtos_to_host.offset as usize,
            RING_RTOS_TO_HOST_OFFSET
        );
        assert!(
            header.rtos_to_host.offset >= header.host_to_rtos.offset + header.host_to_rtos.bytes()
        );

        header.rtos_to_host_head = 5;
        header.rtos_to_host_tail = 3;
        assert_eq!(header.rtos_to_host_pending(), 2);
        header.host_to_rtos_head = 7;
        header.host_to_rtos_tail = 6;
        assert_eq!(header.host_to_rtos_pending(), 1);
    }

    #[test]
    fn header_rejects_wrong_version_or_size() {
        let mut header = ShmHeader::new(1024 * 1024);
        header.version = SHM_VERSION + 1;
        assert!(!header.is_compatible());

        let mut header = ShmHeader::new(1024 * 1024);
        header.size = 256; // 装不下两个环
        assert!(!header.is_compatible());
    }

    #[test]
    fn ring_layout_is_twelve_bytes() {
        assert_eq!(core::mem::size_of::<RingLayout>(), 12);
    }

    #[test]
    fn entry_round_trip_preserves_payload() {
        let mut snapshot = StateSnapshot::default();
        snapshot.seq = 7;
        snapshot.wheel_counts = [11, -22];
        snapshot.duty_permille = [500, 250];
        snapshot.flags = snapshot_flags::ESTOP;

        let bytes = unsafe {
            core::slice::from_raw_parts(
                (&snapshot as *const StateSnapshot) as *const u8,
                core::mem::size_of::<StateSnapshot>(),
            )
        };
        let entry = RingEntry::encode(MessageKind::StateSnapshot, 7, bytes).unwrap();
        let (kind, seq, payload) = entry.decode().unwrap();
        assert_eq!(kind, MessageKind::StateSnapshot);
        assert_eq!(seq, 7);
        assert_eq!(payload.len(), core::mem::size_of::<StateSnapshot>());
        let decoded =
            unsafe { core::ptr::read_unaligned(payload.as_ptr() as *const StateSnapshot) };
        assert_eq!(decoded, snapshot);
    }

    #[test]
    fn entry_decode_rejects_corruption() {
        let entry = RingEntry::encode(MessageKind::Heartbeat, 1, &[1, 2, 3]).unwrap();
        let mut broken = entry;
        broken.payload[0] ^= 0xff;
        assert!(broken.decode().is_none());

        let mut unknown = entry;
        unknown.kind = 99;
        assert!(unknown.decode().is_none());
    }

    #[test]
    fn crc32_matches_known_vector() {
        // 标准测试向量："123456789" → 0xCBF43926
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }
}
