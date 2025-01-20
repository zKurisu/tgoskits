use crate::{irq, time::TimerData};

#[derive(Default)]
pub struct PerCPU {
    pub irq_chips: irq::CpuIrqChips,
    pub timer: TimerData,
}
