use core::time::Duration;

use crate::{
    driver_manager::{
        self,
        device::{BorrowGuard, Device},
        manager,
    },
    globals::{cpu_global, cpu_global_mut, global_val},
    platform_if::*,
};
use driver_interface::timer::*;

#[derive(Default)]
pub(crate) struct Timer {
    timer: Option<Device<PerCPU>>,
}

pub fn since_boot() -> Duration {
    let current_tick = PlatformImpl::current_ticks();
    let freq = PlatformImpl::tick_hz();
    Duration::from_nanos(current_tick * 1_000_000_000 / freq)
}

pub(crate) fn main_cpu_init() {
    match &global_val().platform_info {
        crate::globals::PlatformInfoKind::DeviceTree(fdt) => {
            if let Err(e) = driver_manager::init_timer_by_fdt(fdt.get_addr()) {
                error!("{}", e);
            }
        }
    }

    init_current_cpu();
}

pub(crate) fn init_current_cpu() {
    let timer = manager().timer.get_cpu_timer();
    unsafe { cpu_global_mut().timer.timer = timer };
}

fn timer_write() -> Option<BorrowGuard<PerCPU>> {
    Some(cpu_global().timer.timer.as_ref()?.spin_use("Kernel"))
}

pub fn enable() {
    PlatformImpl::timer_set_interval(10000);
    PlatformImpl::timer_set_irq(true);
    PlatformImpl::timer_set_enable(true);
}
