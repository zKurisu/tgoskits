use core::{error::Error, fmt::Debug};

use crate::{DriverGeneric, custom_type};
use alloc::boxed::Box;

custom_type!(IrqId, usize, "{:#x}");
custom_type!(CpuId, usize, "{:#x}");

pub type Hardware = Box<dyn Interface>;
pub type OnProbeFdt = fn(crate::fdt::Node<'_>) -> Result<Hardware, Box<dyn Error>>;
pub type HardwareCPU = Box<dyn InterfaceCPU>;

pub trait InterfaceCPU: Send + Sync {
    fn get_and_acknowledge_interrupt(&mut self) -> Option<IrqId>;
    fn end_interrupt(&mut self, irq: IrqId);
    fn parse_fdt_config(&self, prop_interrupts: &[u32]) -> Result<IrqConfig, Box<dyn Error>>;
}

pub trait Interface: DriverGeneric {
    fn current_cpu_setup(&self) -> HardwareCPU;
    fn irq_enable(&mut self, irq: IrqId);
    fn irq_disable(&mut self, irq: IrqId);
    fn set_priority(&mut self, irq: IrqId, priority: usize);
    fn set_trigger(&mut self, irq: IrqId, trigger: Trigger);
    fn set_target_cpu(&mut self, irq: IrqId, cpu: CpuId);
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
