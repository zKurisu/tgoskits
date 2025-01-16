use crate::{DriverGeneric, RegAddress};
use alloc::boxed::Box;
use alloc::vec::Vec;

pub type IrqId = usize;

pub type BoxedDriver = Box<dyn InterruptController>;

pub type ProbeFn = fn(regs: Vec<RegAddress>) -> BoxedDriver;

pub trait InterruptController: DriverGeneric {
    fn current_cpu_setup(&self);
    fn get_and_acknowledge_interrupt(&self) -> Option<IrqId>;
    fn end_interrupt(&self, irq: IrqId);
    fn irq_max_size(&self) -> usize;
    fn irq_enable(&mut self, irq: IrqId);
    fn irq_disable(&mut self, irq: IrqId);
    fn set_priority(&mut self, irq: IrqId, priority: usize);
    fn set_trigger(&mut self, irq: IrqId, triger: Trigger);
    fn set_bind_cpu(&mut self, irq: IrqId, cpu_list: &[u64]);
    fn fdt_parse_config(&self, prop_interupt: &[usize]) -> ProbeIrqConfig;
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

#[derive(Clone)]
pub struct ProbeIrqConfig {
    pub irq: IrqId,
    pub trigger: Trigger,
}
