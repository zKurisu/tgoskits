use core::fmt::Debug;

use crate::{custom_type, err::*, DriverGeneric, RegAddress};
use alloc::{boxed::Box, vec::Vec};

custom_type!(IrqId, usize);
custom_type!(CpuId, usize);

pub type BoxedDriver = Box<dyn InterruptController>;

pub type ProbeFn = fn(regs: Vec<RegAddress>) -> BoxedDriver;

pub trait InterruptControllerPerCpu: Send {
    fn get_and_acknowledge_interrupt(&self) -> Option<IrqId>;
    fn end_interrupt(&self, irq: IrqId);
    fn irq_enable(&self, irq: IrqId);
    fn irq_disable(&self, irq: IrqId);
    fn set_priority(&self, irq: IrqId, priority: usize);
    fn set_trigger(&self, irq: IrqId, triger: Trigger);
    fn set_bind_cpu(&self, irq: IrqId, cpu_list: &[CpuId]);
}

pub trait InterruptController: DriverGeneric {
    fn current_cpu_setup(&self) -> Box<dyn InterruptControllerPerCpu>;
    fn parse_fdt_config(&self, prop_interupt: &[usize]) -> DriverResult<IrqConfig>;
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
pub struct IrqConfig {
    pub irq: IrqId,
    pub trigger: Trigger,
}
