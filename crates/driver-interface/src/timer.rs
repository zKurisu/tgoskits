use core::time::Duration;

use alloc::boxed::Box;

pub trait Driver: super::DriverGeneric {
    fn set_interval(&mut self, ticks: u64);
    fn current_ticks(&self) -> u64;
    fn tick_hz(&self) -> u64;
    fn set_enable(&mut self, enable: bool);
    fn set_irq_enable(&mut self, enable: bool);
    fn read_irq_status(&self) -> bool;
    fn irq_num(&self) -> u64;
}

pub type BoxDriver = Box<dyn Driver>;
