//! Diagnostic-only per-stage accounting for the user page-fault path.
//!
//! The default (non-THP) fault path is two orders of magnitude slower than an
//! explicit huge-page fault on SG2002; attributing that gap needs the cost of
//! each stage (plan / prepare / apply / TLB flush / MMU cache update) rather
//! than one aggregate number.  Counters are cumulative; read
//! `/proc/fault_attrib` before and after a workload and subtract.
//!
//! Costs a few `rdtime` reads per fault, so this is a bring-up diagnostic, not
//! something to keep in a shipped build.

use alloc::string::String;
use alloc::format;
use core::sync::atomic::{AtomicU64, Ordering};

pub const STAGE_PLAN: usize = 0;
pub const STAGE_PREPARE: usize = 1;
pub const STAGE_APPLY: usize = 2;
pub const STAGE_TLB: usize = 3;
pub const STAGE_MMU_CACHE: usize = 4;
pub const STAGE_FRAME_ALLOC: usize = 5;
pub const STAGE_FRAME_ZERO: usize = 6;
pub const STAGE_MATERIALIZE: usize = 7;
pub const STAGE_DEPOSIT_PREP: usize = 8;
pub const STAGE_PAGE_OBJECT: usize = 9;
pub const STAGE_PENDING_INSERT: usize = 10;
pub const STAGE_APPLY_PREP: usize = 11;
pub const STAGE_APPLY_MAP: usize = 12;
pub const STAGE_APPLY_PUBLISH: usize = 13;
pub const STAGE_FAULT_AROUND: usize = 14;
/// 顺序缺页预取窗口内部的阶段（`STAGE_FA_APPLY` 是 `FA_BACKEND`/`FA_PUBLISH`
/// 的超集，和 `materialize` 与 `page_object` 的关系一样）。
pub const STAGE_FA_CHECK: usize = 15;
pub const STAGE_FA_PREIMAGE: usize = 16;
pub const STAGE_FA_APPLY: usize = 17;
pub const STAGE_FA_BACKEND: usize = 18;
pub const STAGE_FA_PUBLISH: usize = 19;
pub const STAGE_FA_COMMIT: usize = 20;
/// COW 页索引插入内部的细分（诊断 `pending_insert` 为什么随区域增长）。
pub const STAGE_PEND_LOCK: usize = 21;
pub const STAGE_PEND_SEARCH: usize = 22;
pub const STAGE_PEND_OVERLAP: usize = 23;
pub const STAGE_PEND_STORE: usize = 24;
/// `PageObject` 创建的两半：帧租约 vs 对象本体。
pub const STAGE_PO_LEASE: usize = 25;
pub const STAGE_PO_NEW: usize = 26;
const STAGES: usize = 27;

static TOTALS: [AtomicU64; STAGES] = [const { AtomicU64::new(0) }; STAGES];
static FAULTS: AtomicU64 = AtomicU64::new(0);

/// 有序索引插入的位置直方图（诊断 COW 页索引的搬移代价）。
///
/// 分桶：0 = 插在最前（要搬走整个数组）、1 = 追加在尾部（不用搬）、
/// 2..5 = 距离尾部 16/64/256/更多 个条目。`SKIPPED_ENTRIES` 是累计被搬移
/// 的条目数，乘以条目大小就是这段真正搬了多少字节。
static INSERT_POSITIONS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];
static INSERT_SHIFTED_ENTRIES: AtomicU64 = AtomicU64::new(0);

/// 匿名批量缺页的整块分配结果：成功了多少页、回退逐页了多少页、试了几次。
static ANON_BLOCK_PAGES_OK: AtomicU64 = AtomicU64::new(0);
static ANON_BLOCK_FALLBACK_PAGES: AtomicU64 = AtomicU64::new(0);
static ANON_BLOCK_ATTEMPTS: AtomicU64 = AtomicU64::new(0);

/// Records one contiguous-block attempt's outcome.
pub fn note_anon_block(pages: u64) {
    ANON_BLOCK_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    ANON_BLOCK_PAGES_OK.fetch_add(pages, Ordering::Relaxed);
}

/// Records `pages` filled by the per-page fallback.
pub fn note_anon_block_fallback(pages: u64) {
    ANON_BLOCK_FALLBACK_PAGES.fetch_add(pages, Ordering::Relaxed);
}

/// Records one ordered-index insertion's shift distance.
pub fn note_insert_position(position: usize, len: usize) {
    let shifted = len.saturating_sub(position);
    let bucket = if position == 0 {
        0
    } else if position == len {
        1
    } else if shifted <= 16 {
        2
    } else if shifted <= 64 {
        3
    } else if shifted <= 256 {
        4
    } else {
        5
    };
    INSERT_POSITIONS[bucket].fetch_add(1, Ordering::Relaxed);
    INSERT_SHIFTED_ENTRIES.fetch_add(shifted as u64, Ordering::Relaxed);
}

/// Reads the monotonic clock used for the deltas.
pub fn stage_now() -> u64 {
    ax_hal::time::monotonic_time_nanos()
}

/// Adds one stage's elapsed nanoseconds.
pub fn add(stage: usize, ns: u64) {
    TOTALS[stage].fetch_add(ns, Ordering::Relaxed);
}

/// Counts one resolved (or attempted) fault.
pub fn note_fault() {
    FAULTS.fetch_add(1, Ordering::Relaxed);
}

/// Counts `pages` resolved pages at once.  The sequential fault-around path
/// publishes a whole window from one trap, so counting pages (not traps) keeps
/// every `<stage>_avg` in the table a per-page figure.
pub fn note_faults(pages: u64) {
    FAULTS.fetch_add(pages, Ordering::Relaxed);
}

/// Renders the cumulative per-stage table (for `/proc/fault_attrib`).
pub fn render() -> String {
    let faults = FAULTS.load(Ordering::Relaxed);
    let mut total = 0u64;
    let mut out = String::new();
    for (name, stage) in [
        ("plan", STAGE_PLAN),
        ("prepare", STAGE_PREPARE),
        ("apply", STAGE_APPLY),
        ("tlb", STAGE_TLB),
        ("mmu_cache", STAGE_MMU_CACHE),
        ("frame_alloc", STAGE_FRAME_ALLOC),
        ("frame_zero", STAGE_FRAME_ZERO),
        ("materialize", STAGE_MATERIALIZE),
        ("deposit_prep", STAGE_DEPOSIT_PREP),
        ("page_object", STAGE_PAGE_OBJECT),
        ("pending_insert", STAGE_PENDING_INSERT),
        ("apply_prep", STAGE_APPLY_PREP),
        ("apply_map", STAGE_APPLY_MAP),
        ("apply_publish", STAGE_APPLY_PUBLISH),
        ("fault_around", STAGE_FAULT_AROUND),
        ("fa_check", STAGE_FA_CHECK),
        ("fa_preimage", STAGE_FA_PREIMAGE),
        ("fa_apply", STAGE_FA_APPLY),
        ("fa_backend", STAGE_FA_BACKEND),
        ("fa_publish", STAGE_FA_PUBLISH),
        ("fa_commit", STAGE_FA_COMMIT),
        ("pend_lock", STAGE_PEND_LOCK),
        ("pend_search", STAGE_PEND_SEARCH),
        ("pend_overlap", STAGE_PEND_OVERLAP),
        ("pend_store", STAGE_PEND_STORE),
        ("po_lease", STAGE_PO_LEASE),
        ("po_new", STAGE_PO_NEW),
    ] {
        let ns = TOTALS[stage].load(Ordering::Relaxed);
        total += ns;
        let avg = if faults == 0 { 0 } else { ns / faults };
        out.push_str(&format!("{name}_ns={ns} {name}_avg={avg}\n"));
    }
    out.push_str(&format!(
        "total_ns={total} total_avg={}\nfaults={faults}\n",
        if faults == 0 { 0 } else { total / faults }
    ));
    let (reclaim_calls, reclaim_ns) = ax_alloc::reclaim_stats();
    out.push_str(&format!(
        "reclaim_calls={reclaim_calls} reclaim_ns={reclaim_ns} reclaim_avg={}\n",
        if reclaim_calls == 0 { 0 } else { reclaim_ns / reclaim_calls }
    ));
    let positions: [u64; 6] = core::array::from_fn(|i| INSERT_POSITIONS[i].load(Ordering::Relaxed));
    let shifted = INSERT_SHIFTED_ENTRIES.load(Ordering::Relaxed);
    let inserts: u64 = positions.iter().sum();
    out.push_str(&format!(
        "insert_total={inserts} insert_front={} insert_append={} insert_near16={} \
         insert_near64={} insert_near256={} insert_far={} insert_shifted_entries={shifted} \
         insert_shifted_per_insert={}\n",
        positions[0], positions[1], positions[2], positions[3], positions[4], positions[5],
        if inserts == 0 { 0 } else { shifted / inserts }
    ));
    out.push_str(&format!(
        "anon_block_attempts={} anon_block_pages_ok={} anon_block_fallback_pages={} \
         anon_block_hit_ratio={}\n",
        ANON_BLOCK_ATTEMPTS.load(Ordering::Relaxed),
        ANON_BLOCK_PAGES_OK.load(Ordering::Relaxed),
        ANON_BLOCK_FALLBACK_PAGES.load(Ordering::Relaxed),
        {
            let ok = ANON_BLOCK_PAGES_OK.load(Ordering::Relaxed);
            let fallback = ANON_BLOCK_FALLBACK_PAGES.load(Ordering::Relaxed);
            if ok + fallback == 0 {
                0
            } else {
                ok * 100 / (ok + fallback)
            }
        }
    ));
    out
}
