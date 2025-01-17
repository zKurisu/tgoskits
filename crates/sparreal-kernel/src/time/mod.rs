use core::time::Duration;

use crate::platform_if::*;

pub fn since_boot() -> Duration {
    let current_tick = PlatformImpl::current_ticks();
    let freq = PlatformImpl::tick_hz();
    Duration::from_nanos(current_tick * 1_000_000_000 / freq)
}

pub fn enable() {
    PlatformImpl::timer_set_interval(10000);
    PlatformImpl::timer_set_irq(true);
    PlatformImpl::timer_set_enable(true);
}
