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
const STAGES: usize = 11;

static TOTALS: [AtomicU64; STAGES] = [const { AtomicU64::new(0) }; STAGES];
static FAULTS: AtomicU64 = AtomicU64::new(0);

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
    out
}
