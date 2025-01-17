use core::{cell::UnsafeCell, time::Duration};

use crate::{
    driver_manager::{device::Device, manager},
    globals::{cpu_global, cpu_global_mut},
    platform_if::*,
};
use driver_interface::timer::*;
use log::debug;

#[derive(Default)]
pub(crate) struct Timer {
    timer: Option<Device<PerCPU>>,
}

pub fn since_boot() -> Duration {
    let current_tick = PlatformImpl::current_ticks();
    let freq = PlatformImpl::tick_hz();
    Duration::from_nanos(current_tick * 1_000_000_000 / freq)
}

pub(crate) fn init_current_cpu() {
    let timer = manager().timer.get_cpu_timer();
    unsafe { cpu_global_mut().timer.timer = timer };
}

pub fn enable() {
    PlatformImpl::timer_set_interval(10000);
    PlatformImpl::timer_set_irq(true);
    PlatformImpl::timer_set_enable(true);
}
