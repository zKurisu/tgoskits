use alloc::{boxed::Box, vec::Vec};

pub use crate::IrqProbeConfig;

pub trait Driver: super::DriverGeneric {
    fn get_and_acknowledge_interrupt(&self) -> Option<usize>;
    fn end_interrupt(&self, irq_id: usize);
    fn irq_max_size(&self) -> usize;
    fn enable_irq(&mut self, config: IrqConfig);
    fn disable_irq(&mut self, irq_id: usize);
    fn current_cpu_setup(&self);
    fn fdt_itr_to_config(&self, itr: &[usize]) -> IrqProbeConfig;
}

pub type BoxDriver = Box<dyn Driver>;

#[derive(Debug, Clone)]
pub struct IrqConfig {
    pub irq_id: usize,
    pub trigger: Trigger,
    pub priority: usize,
    pub cpu_list: Vec<usize>,
}

/// The trigger configuration for an interrupt.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Trigger {
    EdgeBoth,
    EdgeRising,
    EdgeFailling,
    LevelHigh,
    LevelLow,
}
