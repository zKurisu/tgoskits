mod guard;

pub use guard::*;

pub fn register_handler<F>(irq: usize, _handler: F)
where
    F: Fn() + Send + Sync + 'static,
{
    crate::hal::al::platform::irq_set_enabled(irq, true);
}
