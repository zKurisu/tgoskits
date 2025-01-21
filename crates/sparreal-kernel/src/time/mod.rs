use core::time::Duration;

use crate::{
    driver_manager::{
        self,
        device::{BorrowGuard, Device},
        manager,
    },
    globals::{cpu_global, cpu_global_meybeuninit, cpu_global_mut, global_val},
    irq::{IrqHandleResult, IrqParam},
};

use driver_interface::{interrupt_controller::IrqId, timer::*};
use log::error;

#[derive(Default)]
pub(crate) struct TimerData {
    timer: Option<Device<Timer>>,
}

pub fn since_boot() -> Duration {
    _since_boot().unwrap_or_default()
}

fn _since_boot() -> Option<Duration> {
    Some(cpu_global_meybeuninit()?.timer.timer.as_ref()?.since_boot())
}

pub(crate) fn init_main_cpu() {
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
    Some(timer_data().timer.as_ref()?.spin_try_use("Kernel"))
}
fn irq_handle(_irq: IrqId) -> IrqHandleResult {
    let t = unsafe { &mut *timer_data().timer.as_ref().unwrap().force_use() };
    t.handle_irq();
    IrqHandleResult::Handled
}

fn timer_data() -> &'static TimerData {
    &cpu_global().timer
}

pub fn after(duration: Duration, call: impl Fn() + 'static) {
    if let Some(mut t) = timer_write() {
        t.after(duration, call);
    }
}

pub fn spin_delay(duration: Duration) {
    let now = since_boot();
    let at = now + duration;

    loop {
        if since_boot() >= at {
            break;
        }
    }
}

pub fn sleep(duration: Duration) {
    let pid = crate::task::current().pid;
    after(duration, move || {
        crate::task::wake_up_in_irq(pid);
    });
    crate::task::suspend();
}
