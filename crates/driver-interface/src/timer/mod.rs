use core::time::Duration;

use crate::{interrupt_controller::IrqConfig, DriverGeneric};
use alloc::{boxed::Box, vec::Vec};

mod queue;

pub type Hardware = Box<dyn Interface>;
pub type ProbeFn = fn(Vec<IrqConfig>) -> Hardware;
pub type HardwareCPU = Box<dyn InterfaceCPU>;
const NANO_PER_SEC: u128 = 1_000_000_000;

pub trait Interface: Send {
    fn get_current_cpu(&mut self) -> Box<dyn InterfaceCPU>;
}

pub trait InterfaceCPU: DriverGeneric + Sync {
    fn set_timeval(&mut self, ticks: u64);
    fn current_ticks(&self) -> u64;
    fn tick_hz(&self) -> u64;
    fn set_irq_enable(&mut self, enable: bool);
    fn read_irq_status(&self) -> bool;
    fn irq(&self) -> IrqConfig;
}

pub struct Timer {
    timer: HardwareCPU,
    q: queue::Queue,
}

unsafe impl Sync for Timer {}

impl Timer {
    pub fn new(timer: HardwareCPU) -> Self {
        Self {
            timer,
            q: queue::Queue::new(),
        }
    }

    pub fn enable(&mut self) {
        let _ = self.timer.open();
    }

    pub fn since_boot(&self) -> Duration {
        self.tick_to_duration(self.timer.current_ticks())
    }

    pub fn after(&mut self, duration: Duration, callback: impl Fn() + 'static) {
        let ticks = self.duration_to_tick(duration);

        let event = queue::Event {
            interval: None,
            at_tick: self.timer.current_ticks() + ticks,
            callback: Box::new(callback),
            called: false,
        };

        self.add_event(event);
    }

    pub fn every(&mut self, duration: Duration, callback: impl Fn() + 'static) {
        let ticks = self.duration_to_tick(duration);

        let event = queue::Event {
            interval: Some(ticks),
            at_tick: self.timer.current_ticks() + ticks,
            callback: Box::new(callback),
            called: false,
        };

        self.add_event(event);
    }

    fn add_event(&mut self, event: queue::Event) {
        self.timer.set_irq_enable(false);

        let next_tick = self.q.add_and_next_tick(event);

        self.timer.set_timeval(next_tick);

        self.timer.set_irq_enable(true);
    }

    pub fn handle_irq(&mut self) {
        while let Some(event) = self.q.pop(self.timer.current_ticks()) {
            (event.callback)();
        }

        match self.q.next_tick() {
            Some(next_tick) => {
                self.timer.set_timeval(next_tick);
            }
            None => {
                self.timer.set_irq_enable(false);
            }
        }
    }

    pub fn set_irq_enable(&mut self, enable: bool) {
        self.timer.set_irq_enable(enable);
    }

    fn tick_to_duration(&self, tick: u64) -> Duration {
        Duration::from_nanos((tick as u128 * NANO_PER_SEC / self.timer.tick_hz() as u128) as _)
    }

    fn duration_to_tick(&self, duration: Duration) -> u64 {
        (duration.as_nanos() * self.timer.tick_hz() as u128 / NANO_PER_SEC) as _
    }

    pub fn irq(&self) -> IrqConfig {
        self.timer.irq()
    }
}
