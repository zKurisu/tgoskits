use crate::{arch::Plat, common::PlatOp};

pub use someboot::irq::*;

pub fn irq_set_enable(irq: rdrive::IrqId, enable: bool) {
    debug!("Setting IRQ {:?} enable to {}", irq, enable);
    Plat::irq_set_enable(irq, enable);
}

pub fn systick_irq() -> rdrive::IrqId {
    Plat::systick_irq()
}


