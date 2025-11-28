use alloc::{boxed::Box, collections::BTreeMap, vec::Vec};
use core::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use crate::os::sync::IrqSpinlock;

const NS_PER_SEC: u64 = 1_000_000_000;

type TimerCallback = Box<dyn FnMut() + Send + 'static>;

static TIMER_MANAGER: IrqSpinlock<Option<TimerManager>> = IrqSpinlock::new(None);
static TIMER_READY: AtomicBool = AtomicBool::new(false);

pub type TimerResult<T> = core::result::Result<T, TimerError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerError {
    NotReady,
    Overflow,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct TimerHandle(TimerId);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
struct TimerId(u64);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TimerKey {
    deadline: Duration,
    id: TimerId,
}

#[derive(Debug, Clone, Copy)]
pub struct TimeListEntry {
    pub handle: TimerHandle,
    pub deadline: Duration,
    pub remaining: Duration,
}

/// Software timer core that keeps a sorted list of one-shot callbacks.
struct TimerManager {
    tick_period: Duration,
    now: Duration,
    next_id: u64,
    timers: BTreeMap<TimerKey, TimerCallback>,
    index: BTreeMap<TimerId, Duration>,
}

impl TimerManager {
    fn new(tick_period: Duration) -> Self {
        assert!(tick_period > Duration::ZERO, "tick period must be non-zero");
        Self {
            tick_period,
            now: Duration::ZERO,
            next_id: 1,
            timers: BTreeMap::new(),
            index: BTreeMap::new(),
        }
    }

    fn schedule_after<F>(&mut self, delay: Duration, callback: F) -> TimerResult<TimerHandle>
    where
        F: FnOnce() + Send + 'static,
    {
        let deadline = self.now.checked_add(delay).ok_or(TimerError::Overflow)?;
        let h = self.schedule_at(deadline, callback);
        Ok(h)
    }

    fn schedule_at<F>(&mut self, deadline: Duration, callback: F) -> TimerHandle
    where
        F: FnOnce() + Send + 'static,
    {
        let id = self.next_timer_id();
        let key = TimerKey { deadline, id };
        self.timers.insert(key, into_callback(callback));
        self.index.insert(id, deadline);
        TimerHandle(id)
    }

    fn cancel(&mut self, handle: TimerHandle) -> bool {
        if let Some(deadline) = self.index.remove(&handle.0) {
            let key = TimerKey {
                deadline,
                id: handle.0,
            };
            self.timers.remove(&key);
            return true;
        }
        false
    }

    fn handle_irq(&mut self) -> Vec<TimerCallback> {
        self.now = self
            .now
            .checked_add(self.tick_period)
            .unwrap_or(Duration::MAX);

        let mut expired = Vec::new();
        loop {
            let Some(key) = self.timers.keys().next().cloned() else {
                break;
            };
            if key.deadline > self.now {
                break;
            }
            if let Some(cb) = self.timers.remove(&key) {
                expired.push(cb);
            }
            self.index.remove(&key.id);
        }
        expired
    }

    fn snapshot(&self) -> Vec<TimeListEntry> {
        let mut list = Vec::with_capacity(self.timers.len());
        for key in self.timers.keys() {
            let remaining = key.deadline.saturating_sub(self.now);
            list.push(TimeListEntry {
                handle: TimerHandle(key.id),
                deadline: key.deadline,
                remaining,
            });
        }
        list
    }

    fn uptime(&self) -> Duration {
        self.now
    }

    fn next_timer_id(&mut self) -> TimerId {
        loop {
            let id = TimerId(self.next_id);
            self.next_id = self.next_id.wrapping_add(1);
            if !self.index.contains_key(&id) {
                return id;
            }
        }
    }
}

pub(crate) fn init() {
    {
        let mut guard = TIMER_MANAGER.lock();
        if guard.is_some() {
            return;
        }
        *guard = Some(TimerManager::new(default_tick_period()));
    }

    TIMER_READY.store(true, Ordering::Release);

    let timer_irq = crate::hal::al::cpu::systimer_irq();
    crate::os::irq::register_handler(timer_irq, systimer_irq_handler);
    crate::hal::al::cpu::systimer_disable();
    arm_next_tick();
}

/// Schedule a one-shot timer after the provided delay.
pub(crate) fn one_shot_after<F>(delay: Duration, callback: F) -> Result<TimerHandle, TimerError>
where
    F: FnOnce() + Send + 'static,
{
    let mut cb = Some(callback);
    with_manager_mut(|mgr| mgr.schedule_after(delay, cb.take().unwrap()))
        .ok_or(TimerError::NotReady)?
}

/// Schedule a one-shot timer that fires at the absolute deadline.
pub(crate) fn one_shot_at<F>(deadline: Duration, callback: F) -> Result<TimerHandle, TimerError>
where
    F: FnOnce() + Send + 'static,
{
    let mut cb = Some(callback);
    with_manager_mut(|mgr| mgr.schedule_at(deadline, cb.take().unwrap()))
        .ok_or(TimerError::NotReady)
}

pub(crate) fn cancel(handle: TimerHandle) -> bool {
    with_manager_mut(|mgr| mgr.cancel(handle)).unwrap_or(false)
}

/// Monotonic time elapsed since the timer subsystem was initialised.
pub(crate) fn uptime() -> Duration {
    if !TIMER_READY.load(Ordering::Acquire) {
        return Duration::ZERO;
    }
    with_manager(|mgr| mgr.uptime()).unwrap_or(Duration::ZERO)
}

/// Snapshot the current pending timers for diagnostics.
pub fn time_list() -> Vec<TimeListEntry> {
    with_manager(|mgr| mgr.snapshot()).unwrap_or_default()
}

pub fn is_ready() -> bool {
    TIMER_READY.load(Ordering::Acquire)
}

pub fn tick_period() -> Duration {
    with_manager(|mgr| mgr.tick_period).unwrap_or_default()
}

fn systimer_irq_handler() {
    let callbacks = with_manager_mut(|mgr| mgr.handle_irq()).unwrap_or_default();
    arm_next_tick();
    run_callbacks(callbacks);
}

fn run_callbacks(callbacks: Vec<TimerCallback>) {
    for mut cb in callbacks {
        (cb)();
    }
}

fn into_callback<F>(f: F) -> TimerCallback
where
    F: FnOnce() + Send + 'static,
{
    let mut opt = Some(f);
    Box::new(move || {
        if let Some(inner) = opt.take() {
            inner();
        }
    })
}

fn default_tick_period() -> Duration {
    Duration::from_millis(1)
}

fn with_manager<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&TimerManager) -> R,
{
    let guard = TIMER_MANAGER.lock();
    guard.as_ref().map(f)
}

fn with_manager_mut<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut TimerManager) -> R,
{
    let mut guard = TIMER_MANAGER.lock();
    guard.as_mut().map(f)
}

fn arm_next_tick() {
    if !TIMER_READY.load(Ordering::Acquire) {
        return;
    }

    let period = tick_period();
    if period == Duration::ZERO {
        return;
    }

    let ns = duration_to_ns(period);
    if ns == 0 {
        return;
    }

    crate::hal::al::cpu::systimer_set_next_event(ns);
}

fn duration_to_ns(duration: Duration) -> u64 {
    duration
        .as_secs()
        .saturating_mul(NS_PER_SEC)
        .saturating_add(duration.subsec_nanos() as u64)
}
