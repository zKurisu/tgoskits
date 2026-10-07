//! 用户态指令缓存"可能陈旧"的标记（B1 优化用）。
//!
//! 板上实测：`run_unchecked()` 里那次**每次返回用户态都做**的 `fence.i` 要
//! 2.17 µs，占一次空系统调用（8.5–11.6 µs）的 ~19%。可是绝大多数系统调用返回
//! 时用户代码页根本没被动过 —— 只有"内核写过可执行页"（exec 装载、mprotect 加
//! PROT_EXEC、mmap 新映射可执行文件）和"换地址空间"这两种情况下 I-cache 才可能
//! 留着旧内容。Linux 在 RISC-V 上就是这么做的（`icache_stale_mask` +
//! `switch_mm`/`flush_icache_mm`），不是每次返回都刷。
//!
//! 这里只提供"标记 / 取标记 / 决定要不要刷"这三件事；`always_flush` 是为了
//! 在同一块板上做 A/B（默认仍是改前的 eager 行为，量完再决定是否切换默认值）。

use core::sync::atomic::{AtomicBool, Ordering};

/// 当前 CPU 的用户 I-cache 可能含旧内容。
static ICACHE_STALE: AtomicBool = AtomicBool::new(true);
/// 每次进用户态都刷（改动前的行为）。默认关闭：内核已经在"页变成可执行"
/// （`PageObject::prepare_executable_mapping`，所有装 PTE 的路径都会走）和
/// "内核改写了用户正文"（`AddrSpace::sync_modified_text`，ptrace 单步补丁）这两处
/// 刷过 I-cache，再加上 exec / mmap(PROT_EXEC) / mprotect(PROT_EXEC) / 换地址空间
/// 会标脏，逐次返回再刷就是纯冗余。旋钮 `/proc/icache_flush` 可以随时切回 1 做对照。
static ALWAYS_FLUSH: AtomicBool = AtomicBool::new(false);

/// Marks the current CPU's user instruction cache as potentially stale.
pub fn mark_stale() {
    ICACHE_STALE.store(true, Ordering::Release);
}

/// Reads and clears the staleness flag.
pub fn take_stale() -> bool {
    ICACHE_STALE.swap(false, Ordering::AcqRel)
}

/// Peeks at the staleness flag without clearing it (for `/proc` rendering).
pub fn stale() -> bool {
    ICACHE_STALE.load(Ordering::Acquire)
}

/// Whether every user entry flushes unconditionally.
pub fn always_flush() -> bool {
    ALWAYS_FLUSH.load(Ordering::Acquire)
}

/// Selects eager (`true`, pre-optimization) or lazy (`false`) flushing.
pub fn set_always_flush(value: bool) {
    ALWAYS_FLUSH.store(value, Ordering::Release);
}

/// Decides whether this user entry must flush the instruction cache.
#[inline]
pub fn need_flush() -> bool {
    let stale = take_stale();
    always_flush() || stale
}
