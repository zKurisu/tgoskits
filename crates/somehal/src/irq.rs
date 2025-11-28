use crate::ArchTrait;

pub fn systimer_irq() -> usize {
    crate::arch::Arch::systimer_irq()
}

pub fn irq_all_is_enabled() -> bool {
    crate::arch::Arch::irq_all_is_enabled()
}

pub fn irq_all_set_enable(enabled: bool) {
    crate::arch::Arch::irq_all_set_enable(enabled);
}
