use core::time::Duration;

use alloc::boxed::Box;

pub trait Driver: super::DriverGeneric {
    fn set_interval(&mut self, ticks: u64);
    fn current_ticks(&self) -> u64;
    fn tick_hz(&self) -> u64;
    fn set_enable(&mut self, enable: bool);
}

pub type BoxDriver = Box<dyn Driver>;
