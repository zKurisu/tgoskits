use crate::{irq, time::Timer};

#[derive(Default)]
pub struct PerCPU {
    pub irq_chips: irq::CpuIrqChips,
    pub timer: Timer,
}
