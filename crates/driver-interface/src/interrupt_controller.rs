use core::{error::Error, fmt::Debug};

use crate::{custom_type, DriverGeneric, RegAddress};
use alloc::{boxed::Box, vec::Vec};

custom_type!(IrqId, usize, "{:#x}");
custom_type!(CpuId, usize, "{:#x}");

pub type Driver = Box<dyn Interface>;
pub type ProbeFn = fn(regs: Vec<RegAddress>) -> Driver;
pub type DriverCPU = Box<dyn InterfaceCPU>;

pub trait InterfaceCPU: Send {
    fn get_and_acknowledge_interrupt(&self) -> Option<IrqId>;
    fn end_interrupt(&self, irq: IrqId);
    fn irq_enable(&self, irq: IrqId);
    fn irq_disable(&self, irq: IrqId);
    fn set_priority(&self, irq: IrqId, priority: usize);
    fn set_trigger(&self, irq: IrqId, triger: Trigger);
    fn set_bind_cpu(&self, irq: IrqId, cpu_list: &[CpuId]);
    fn parse_fdt_config(&self, prop_interrupts: &[u32]) -> Result<IrqConfig, Box<dyn Error>>;
}

pub trait Interface: DriverGeneric {
    fn current_cpu_setup(&self) -> DriverCPU;
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
