use core::{error::Error, fmt::Debug};

use crate::{interrupt_controller::IrqConfig, DriverGeneric};
use alloc::{boxed::Box, string::String, vec::Vec};

pub type Driver = Box<dyn Interface>;
pub type ProbeFn = fn(Vec<IrqConfig>) -> Driver;
pub type PerCPU = Box<dyn InterfacePerCPU>;

pub trait Interface: Send {
    fn get_current_cpu(&mut self) -> Box<dyn InterfacePerCPU>;
    fn irq(&self) -> IrqConfig;
    fn name(&self) -> String;
}

pub trait InterfacePerCPU: DriverGeneric {
    fn set_interval(&mut self, ticks: u64);
    fn current_ticks(&self) -> u64;
    fn tick_hz(&self) -> u64;
    fn set_irq_enable(&mut self, enable: bool);
    fn read_irq_status(&self) -> bool;
}
