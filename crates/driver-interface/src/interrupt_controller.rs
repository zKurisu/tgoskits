use core::{error::Error, fmt::Debug};

use crate::{custom_type, DriverGeneric, RegAddress};
use alloc::{boxed::Box, vec::Vec};

custom_type!(IrqId, usize);
custom_type!(CpuId, usize);

pub type Driver = Box<dyn InterruptController>;
pub type ProbeFn = fn(regs: Vec<RegAddress>) -> Driver;
pub type PerCPU = Box<dyn InterruptControllerPerCpu>;

pub trait InterruptControllerPerCpu: Send {
    fn get_and_acknowledge_interrupt(&self) -> Option<IrqId>;
    fn end_interrupt(&self, irq: IrqId);
    fn irq_enable(&self, irq: IrqId);
    fn irq_disable(&self, irq: IrqId);
    fn set_priority(&self, irq: IrqId, priority: usize);
    fn set_trigger(&self, irq: IrqId, triger: Trigger);
    fn set_bind_cpu(&self, irq: IrqId, cpu_list: &[CpuId]);
    fn parse_fdt_config(&self, prop_interrupts: &[usize]) -> Result<IrqConfig, Box<dyn Error>>;
}

pub trait InterruptController: DriverGeneric {
    fn current_cpu_setup(&self) -> PerCPU;
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

#[derive(Debug, Clone)]
pub struct IrqConfig {
    pub irq: IrqId,
    pub trigger: Trigger,
}
