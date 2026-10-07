//! C1 诊断：`StagedThread::activate` 内部的分段计时。
//!
//! `fork()` 里 `staged_task.activate()` 单独就要 ~2 ms（同板 Linux 整个 fork 才
//! 0.3 ms），而它只是"把已经建好的线程第一次放进运行队列"。这里把这一段拆开：
//! 运行队列入队、入队收尾（可能触发重调度）、本地定时器重编程、以及 IRQ/抢占
//! 守卫析构（可能在这里发生真正的上下文切换）。
//!
//! 时钟由内核在启动时注册（`ax_task::diag::register_clock`）；宿主测试下没有
//! 时钟 ⇒ 全部退化成 no-op。

use core::sync::atomic::{AtomicU64, Ordering};

pub const STAGE_FINISH_PUBLISHED: usize = 0;
pub const STAGE_ENTER_GUARD: usize = 1;
pub const STAGE_ENQUEUE: usize = 2;
pub const STAGE_FINISH_ENQUEUE: usize = 3;
pub const STAGE_TIMER_PROGRAM: usize = 4;
pub const STAGE_GUARD_DROP: usize = 5;
/// C1：`schedule_current_cpu_with_entry` 的三段（进入 / 决策 / 真正切换）。
/// `sched_switch` 里包含"切到别的任务并在其让出后再切回来"的墙钟时间。
pub const STAGE_SCHED_ENTER: usize = 6;
pub const STAGE_SCHED_DECISION: usize = 7;
pub const STAGE_SCHED_SWITCH: usize = 8;
/// C2：一次上下文切换内部（地址空间激活 → 线程绑定 → 裸切换交接）。
pub const STAGE_SW_MM_PREP: usize = 9;
pub const STAGE_SW_BINDING: usize = 10;
pub const STAGE_SW_ASM: usize = 11;
/// C2：idle 循环里真正进入 `wfi` 的次数与耗时（判断 fork 的那 1.8 ms 是不是"空等"）。
pub const STAGE_IDLE_WAIT: usize = 12;
const STAGES: usize = 13;

const NAMES: [&str; STAGES] = [
    "finish_published",
    "enter_guard",
    "enqueue",
    "finish_enqueue",
    "timer_program",
    "guard_drop",
    "sched_enter",
    "sched_decision",
    "sched_switch",
    "sw_mm_prep",
    "sw_binding",
    "sw_asm",
    "idle_wait",
];

/// 裸切换前的单调时间戳（单核板，跨切换传递一个全局值即可）。
static SWITCH_HANDOFF_START: AtomicU64 = AtomicU64::new(0);

/// Records the instant just before the naked machine transfer.
#[inline]
pub fn note_switch_handoff_start(ns: u64) {
    SWITCH_HANDOFF_START.store(ns, Ordering::Relaxed);
}

/// Attributes the hand-off latency to the incoming context's first instructions.
#[inline]
pub fn note_switch_handoff_end(ns: u64) {
    let start = SWITCH_HANDOFF_START.swap(0, Ordering::Relaxed);
    if start != 0 && ns > start {
        add(STAGE_SW_ASM, ns - start);
    }
}

static TOTALS: [AtomicU64; STAGES] = [const { AtomicU64::new(0) }; STAGES];
static CALLS: [AtomicU64; STAGES] = [const { AtomicU64::new(0) }; STAGES];

/// `execute_switch_plan` 真正执行上下文切换的次数（诊断"每次 activate 到底切了几次"）。
static SWITCHES: AtomicU64 = AtomicU64::new(0);

/// Counts one executed context switch.
#[inline]
pub fn note_switch() {
    SWITCHES.fetch_add(1, Ordering::Relaxed);
}

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
            "task_{}_ns={total} task_{}_avg={} task_{}_calls={calls}\n",
            NAMES[stage],
            NAMES[stage],
            if calls == 0 { 0 } else { total / calls },
            NAMES[stage]
        ));
    }
    out.push_str(&format!(
        "task_switches={}\n",
        SWITCHES.load(Ordering::Relaxed)
    ));
    out
}
