use core::{error::Error, fmt::Debug};

use crate::{custom_type, DriverGeneric, RegAddress};
use alloc::{boxed::Box, vec::Vec};

custom_type!(IrqId, usize, "{:#x}");
custom_type!(CpuId, usize, "{:#x}");

pub type Hardware = Box<dyn Interface>;
pub type ProbeFn = fn(regs: Vec<RegAddress>) -> Hardware;
pub type HardwareCPU = Box<dyn InterfaceCPU>;

pub trait InterfaceCPU: Send {
    fn get_and_acknowledge_interrupt(&mut self) -> Option<IrqId>;
    fn end_interrupt(&mut self, irq: IrqId);
    fn irq_enable(&mut self, irq: IrqId);
    fn irq_disable(&mut self, irq: IrqId);
    fn set_priority(&mut self, irq: IrqId, priority: usize);
    fn set_trigger(&mut self, irq: IrqId, triger: Trigger);
    fn set_bind_cpu(&mut self, irq: IrqId, cpu_list: &[CpuId]);
    fn parse_fdt_config(&self, prop_interrupts: &[u32]) -> Result<IrqConfig, Box<dyn Error>>;
    fn irq_pin_to_id(&self, pin: usize)->Result<IrqId, Box<dyn Error>>;
}

pub trait Interface: DriverGeneric {
    fn current_cpu_setup(&self) -> HardwareCPU;
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
