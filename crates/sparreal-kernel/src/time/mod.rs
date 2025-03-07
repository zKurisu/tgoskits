use core::time::Duration;

use crate::{
    globals::{cpu_global, cpu_global_meybeuninit, cpu_global_mut},
    irq::{IrqHandleResult, IrqParam},
};

use rdrive::{Device, DeviceGuard, intc::IrqId};
pub use timer::Timer;

mod queue;
mod timer;

#[derive(Default)]
pub(crate) struct TimerData {
    timer: Option<Device<Timer>>,
}

pub fn since_boot() -> Duration {
    _since_boot().unwrap_or_default()
}

fn _since_boot() -> Option<Duration> {
    let timer = cpu_global_meybeuninit()?.timer.timer.as_ref()?;
    Some(timer.since_boot())
}

pub(crate) fn init_current_cpu() -> Option<()> {
    {
        let mut ls = rdrive::read(|m| m.timer.all());
        let (_, timer) = ls.pop()?;

        let mut timer = timer.upgrade()?.spin_try_borrow_by(0.into());

        unsafe {
            cpu_global_mut().timer.timer = Some(Device::new(
                timer.descriptor.clone(),
                Timer::new(timer.get_current_cpu()),
            ))
        };
    }
    let mut t = timer_write()?;

    t.set_irq_enable(false);
    t.enable();

    IrqParam {
        intc: t.descriptor.irq_parent?,
        cfg: t.irq(),
    }
    .register_builder(irq_handle)
    .register();

    Some(())
}

fn timer_write() -> Option<DeviceGuard<Timer>> {
    Some(timer_data().timer.as_ref()?.spin_try_borrow_by(0.into()))
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
