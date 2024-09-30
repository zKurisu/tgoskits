use alloc::{boxed::Box, vec::Vec};

use crate::DriverId;
pub use crate::IrqProbeConfig;

pub trait Driver: super::DriverGeneric {
    fn get_and_acknowledge_interrupt(&self) -> Option<usize>;
    fn end_interrupt(&self, irq: usize);
    fn irq_max_size(&self) -> usize;
    fn irq_enable(&mut self, irq: usize);
    fn irq_enable(&mut self, irq: usize);
    fn current_cpu_setup(&self);
    fn set_priority(&mut self, irq: usize, priority: usize);
    fn set_trigger(&mut self, irq: usize, triger: Trigger);
    fn set_bind_cpu(&mut self, irq: usize, cpu_list: &[u64]);
    fn fdt_parse_config(&self, prop_interupt: &[usize]) -> IrqProbeConfig;
}

pub type BoxDriver = Box<dyn Driver>;

/// The trigger configuration for an interrupt.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Trigger {
    EdgeBoth,
    EdgeRising,
    EdgeFailling,
    LevelHigh,
    LevelLow,
}
