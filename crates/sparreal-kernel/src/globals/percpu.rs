use crate::irq;

#[derive(Default)]
pub struct PerCPU {
    pub irq_chips: irq::CpuIrqChips,
}
