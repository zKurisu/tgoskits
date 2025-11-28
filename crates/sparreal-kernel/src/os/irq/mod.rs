mod guard;

use alloc::{boxed::Box, collections::btree_map::BTreeMap};
pub use guard::*;

use crate::os::sync::IrqSpinlock;

static IRQ_VEC: IrqSpinlock<BTreeMap<usize, Box<dyn Fn() + Send + Sync>>> =
    IrqSpinlock::new(BTreeMap::new());

pub fn register_handler<F>(irq: usize, handler: F)
where
    F: Fn() + Send + Sync + 'static,
{
    crate::hal::al::platform::irq_set_enabled(irq, true);
    let mut guard = IRQ_VEC.lock();
    guard.insert(irq, Box::new(handler));
}

pub(crate) fn handle_irq(irq: usize) {
    let guard = IRQ_VEC.lock();
    if let Some(handler) = guard.get(&irq) {
        handler();
    }
}
