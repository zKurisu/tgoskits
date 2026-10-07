//! C2/B1 诊断：返回用户态那条路上的分段计时。
//!
//! `run_unchecked()` 每次返回用户态都会先执行一次 `fence.i`（注释写的是
//! "resolve user program errors"）。getpid 的空系统调用地板在板上是 ~8.5 µs，
//! 而 `fence.i` 在 C906L 上是要刷整条流水线/I-cache 的重操作，所以先把它和
//! 裸的用户往返分开计时，再决定动不动它。
//!
//! 时钟由内核启动时注册（`ax_cpu::diag::register_clock`），没有注册时全部退化成
//! no-op，因此宿主单测/其他平台不受影响。

use core::sync::atomic::{AtomicU64, Ordering};

/// Returning to user mode: the per-entry `fence.i`.
pub const STAGE_FENCE_I: usize = 0;
/// Returning to user mode: the raw `enter_user` round trip (user code + trap).
pub const STAGE_USER_ROUNDTRIP: usize = 1;
/// Returning to user mode: the pre-entry asserts/validation.
pub const STAGE_PREPARE: usize = 2;
const STAGES: usize = 3;

const NAMES: [&str; STAGES] = ["fence_i", "user_roundtrip", "prepare"];

static TOTALS: [AtomicU64; STAGES] = [const { AtomicU64::new(0) }; STAGES];
static CALLS: [AtomicU64; STAGES] = [const { AtomicU64::new(0) }; STAGES];

/// 单调时钟函数指针（0 = 未注册）。
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
    // SAFETY: 只有 `register_clock` 会写这个值，写进去的一定是 `fn() -> u64`。
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
    /// Starts timing `stage`.
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

/// Renders the cumulative per-stage table (for the kernel's `/proc/fault_attrib`).
pub fn render() -> alloc::string::String {
    use alloc::format;
    use alloc::string::String;

    let mut out = String::new();
    for stage in 0..STAGES {
        let total = TOTALS[stage].load(Ordering::Relaxed);
        let calls = CALLS[stage].load(Ordering::Relaxed);
        out.push_str(&format!(
            "cpu_{}_ns={total} cpu_{}_avg={} cpu_{}_calls={calls}\n",
            NAMES[stage], NAMES[stage],
            if calls == 0 { 0 } else { total / calls },
            NAMES[stage]
        ));
    }
    out
}
