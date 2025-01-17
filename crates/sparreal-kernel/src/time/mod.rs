use core::time::Duration;

use crate::{
    driver_manager::{
        self,
        device::{BorrowGuard, Device},
        manager,
    },
    globals::{cpu_global, cpu_global_mut, global_val},
    irq::{IrqHandleResult, IrqParam},
    platform_if::*,
};
use driver_interface::{interrupt_controller::IrqId, timer::*};
use log::error;

#[derive(Default)]
pub(crate) struct TimerData {
    timer: Option<Device<Timer>>,
}

pub fn since_boot() -> Duration {
    let tick = PlatformImpl::current_ticks();
    tick_to_duration(tick)
}

fn tick_to_duration(tick: u64) -> Duration {
    Duration::from_nanos((tick as u128 * 1_000_000_000 / PlatformImpl::tick_hz() as u128) as _)
}

fn duration_to_tick(duration: Duration) -> u64 {
    (duration.as_nanos() * PlatformImpl::tick_hz() as u128 / 1_000_000_000) as _
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

pub(crate) fn init_current_cpu() -> Option<()> {
    let timer = manager().timer.get_cpu_timer();
    unsafe { cpu_global_mut().timer.timer = timer };

    let mut t = timer_write()?;

    t.set_irq_enable(false);
    t.enable();

    let irq_chip = t.descriptor.irq.as_ref()?.irq_parent;

    IrqParam {
        irq_chip,
        cfg: t.irq(),
    }
    .register_builder(irq_handle)
    .register();

    Some(())
}

fn timer_write() -> Option<BorrowGuard<Timer>> {
    Some(timer_data().timer.as_ref()?.spin_use("Kernel"))
}
fn irq_handle(_irq: IrqId) -> IrqHandleResult {
    let t = unsafe { &mut *timer_data().timer.as_ref().unwrap().force_use() };
    t.handle_irq();
    IrqHandleResult::Handled
}

fn timer_data() -> &'static TimerData {
    unsafe { &cpu_global().timer }
}

pub fn after(duration: Duration, call: impl Fn() + 'static) {
    if let Some(mut t) = timer_write() {
        t.after(duration, call);
    }
}
