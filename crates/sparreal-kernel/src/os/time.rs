use core::time::Duration;

pub use crate::hal::timer::{TimeListEntry, TimerError, TimerHandle, TimerResult};

/// Time since the timer subsystem was initialised.
pub fn since_boot() -> Duration {
    crate::hal::timer::uptime()
}

/// Schedule a one-shot callback relative to `now`.
pub fn one_shot_after<F>(delay: Duration, callback: F) -> Result<TimerHandle, TimerError>
where
    F: FnOnce() + Send + 'static,
{
    crate::hal::timer::one_shot_after(delay, callback)
}

/// Schedule a one-shot callback that fires at `deadline`.
pub fn one_shot_at<F>(deadline: Duration, callback: F) -> Result<TimerHandle, TimerError>
where
    F: FnOnce() + Send + 'static,
{
    crate::hal::timer::one_shot_at(deadline, callback)
}

/// Cancel a previously scheduled one-shot timer.
pub fn cancel(handle: TimerHandle) -> bool {
    crate::hal::timer::cancel(handle)
}

/// Inspect pending timers along with remaining time.
pub fn time_list() -> alloc::vec::Vec<TimeListEntry> {
    crate::hal::timer::time_list()
}

/// Check whether the timer subsystem finished initialising.
pub fn is_ready() -> bool {
    crate::hal::timer::is_ready()
}
