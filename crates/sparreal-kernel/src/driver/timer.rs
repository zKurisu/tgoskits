use core::time::Duration;

use alloc::vec::Vec;
pub use driver_interface::timer::*;

use crate::{
    irq::{irq_set_handle, IrqConfig, IrqHandle},
    struct_driver,
};

use super::CONTAINER;

struct_driver!(DriverTimer, BoxDriver);

pub fn list() -> Vec<DriverTimer> {
    let g = CONTAINER.timer.read();
    g.values().map(|one| one.clone()).collect()
}

impl DriverTimer {
    pub fn once_shot(&self, duration: Duration, callback: impl Fn() + 'static) {
        let mut timer = self.spec.write();
        let irq = timer.irq_num() as usize;
        let t = self.clone();

        irq_set_handle(irq, self.desc.id, move |_| {
            let mut timer = t.spec.write();
            if timer.read_irq_status() {
                callback();
                timer.set_irq_enable(false);
                IrqHandle::Handled
            } else {
                IrqHandle::None
            }
        });
        let ticks = timer.tick_hz() as f64 * duration.as_secs_f64();
        timer.set_interval(ticks as _);
        timer.set_enable(true);
        timer.set_irq_enable(true);
    }
}
