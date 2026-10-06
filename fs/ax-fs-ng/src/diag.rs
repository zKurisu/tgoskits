//! H5b 用的只读分段计时（零行为改变）。
//!
//! 板子上 ext4 的 `open+read(0 KB)+close` 中位 ~10 ms，而 procfs 只要 0.4 ms，
//! 且与数据量无关、每次重复都一样 —— 单靠系统调用级的时间没法知道钱花在
//! 路径解析、inode 构建、缓存身份还是页填充上。这里按段累计纳秒与次数：
//!
//! * `open_get_or_create`：`CachedFile::get_or_create` 全过程；
//! * `open_register`：身份发布 + `register_cached_file`（含注册表裁剪）；
//! * `read_populate`：`populate_page_window`（页缓存未命中时的落盘填充）；
//! * `read_copy`：页内容拷到 scratch + 拷到用户缓冲；
//! * `ext4_inode_new`：ext4 每次 lookup 构造 `Inode` 的开销；
//! * `ext4_lookup`：ext4 目录项查找。
//!
//! 时钟由内核在启动时注册（`ax_fs_ng::diag::register_clock`），宿主单测下没有
//! 时钟 ⇒ 全部退化成 no-op，不影响测试。

use core::sync::atomic::{AtomicU64, Ordering};

pub const STAGE_OPEN_GET_OR_CREATE: usize = 0;
pub const STAGE_OPEN_REGISTER: usize = 1;
pub const STAGE_READ_POPULATE: usize = 2;
pub const STAGE_READ_COPY: usize = 3;
pub const STAGE_EXT4_INODE_NEW: usize = 4;
pub const STAGE_EXT4_LOOKUP: usize = 5;
const STAGES: usize = 6;

const NAMES: [&str; STAGES] = [
    "open_get_or_create",
    "open_register",
    "read_populate",
    "read_copy",
    "ext4_inode_new",
    "ext4_lookup",
];

static TOTALS: [AtomicU64; STAGES] = [const { AtomicU64::new(0) }; STAGES];
static CALLS: [AtomicU64; STAGES] = [const { AtomicU64::new(0) }; STAGES];

/// 单调时钟函数指针（0 = 未注册；宿主测试下就是 0）。
static CLOCK: AtomicU64 = AtomicU64::new(0);

/// Registers the monotonic clock the kernel uses for this diagnostic.
pub fn register_clock(clock: fn() -> u64) {
    CLOCK.store(clock as usize as u64, Ordering::Release);
}

#[inline]
fn now() -> u64 {
    let raw = CLOCK.load(Ordering::Acquire);
    if raw == 0 {
        return 0;
    }
    // SAFETY: 只有 `register_clock` 会写入这个值，写进来的一定是 `fn() -> u64`
    // 的地址（`fn` 指针非空、表示可直接调用）。
    let clock: fn() -> u64 = unsafe { core::mem::transmute(raw as usize) };
    clock()
}

/// Adds one stage's elapsed nanoseconds.
#[inline]
pub fn add(stage: usize, ns: u64) {
    if ns != 0 {
        TOTALS[stage].fetch_add(ns, Ordering::Relaxed);
        CALLS[stage].fetch_add(1, Ordering::Relaxed);
    }
}

/// Timestamps the enclosing scope and attributes it to `stage` on drop.
#[must_use]
pub struct Scope {
    stage: usize,
    start: u64,
}

impl Scope {
    #[inline]
    pub fn new(stage: usize) -> Self {
        Self {
            stage,
            start: now(),
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let end = now();
        if end > self.start {
            add(self.stage, end - self.start);
        }
    }
}

/// Convenience constructor: `let _t = diag::scope(diag::STAGE_X);`
#[inline]
pub fn scope(stage: usize) -> Scope {
    Scope::new(stage)
}

/// Renders the cumulative per-stage table (for `/proc/fault_attrib`).
pub fn render() -> alloc::string::String {
    use alloc::format;
    use alloc::string::String;

    let mut out = String::new();
    for stage in 0..STAGES {
        let total = TOTALS[stage].load(Ordering::Relaxed);
        let calls = CALLS[stage].load(Ordering::Relaxed);
        out.push_str(&format!(
            "fs_{}_ns={total} fs_{}_avg={}\n",
            NAMES[stage],
            NAMES[stage],
            if calls == 0 { 0 } else { total / calls }
        ));
    }
    out
}
