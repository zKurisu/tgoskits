use crate::ArchTrait;

pub fn systimer_irq() -> usize {
    crate::arch::Arch::systimer_irq()
}

pub fn irq_local_is_enabled() -> bool {
    crate::arch::Arch::irq_all_is_enabled()
}

pub fn irq_local_set_enable(enabled: bool) {
    crate::arch::Arch::irq_all_set_enable(enabled);
}

pub fn irq_is_enabled(irq: usize) -> bool {
    crate::arch::Arch::irq_is_enabled(SoftIrqId(irq))
}

pub fn irq_set_enable(irq: usize, enable: bool) {
    crate::arch::Arch::irq_set_enable(SoftIrqId(irq), enable);
}

#[repr(transparent)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SoftIrqId(usize);

impl SoftIrqId {
    pub const fn new(id: usize) -> Self {
        SoftIrqId(id)
    }

    pub const fn raw(&self) -> usize {
        self.0
    }
}
