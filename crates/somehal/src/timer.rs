use crate::ArchTrait;

/// Enable the platform system timer so that timer IRQs can fire.
pub fn enable() {
    crate::arch::Arch::systimer_enable();
}

/// Disable the platform system timer to stop timer IRQs.
pub fn disable() {
    crate::arch::Arch::systimer_disable();
}

/// Configure the system timer with the desired interval in nanoseconds.
pub fn set_next_event(interval_ns: u64) {
    crate::arch::Arch::systimer_set_next_event(interval_ns);
}
