use crate::common::PlatOp;

pub struct Plat;

impl PlatOp for Plat {
    fn init_irq_main() -> Result<(), anyhow::Error> {
        Ok(())
    }

    fn init_irq_current_cpu() -> Result<(), anyhow::Error> {
        Ok(())
    }

    fn irq_set_enable(irq: rdrive::IrqId, enable: bool) {}

    fn systick_irq() -> rdrive::IrqId {
        someboot::irq::systimer_irq().raw().into()
    }
}
