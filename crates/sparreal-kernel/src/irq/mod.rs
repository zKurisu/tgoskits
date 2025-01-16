use core::cell::UnsafeCell;

use alloc::{boxed::Box, collections::btree_map::BTreeMap};
use driver_interface::interrupt_controller;

use crate::{driver_manager, globals};

#[derive(Default)]
pub struct CpuIrqChips(BTreeMap<usize, Box<dyn interrupt_controller::InterruptControllerPerCpu>>);

pub(crate) fn init_current_cpu() {
    let chip = driver_manager::interrupt_controllers();
    let g = unsafe { globals::cpu_global_mut() };

    for (id, c) in chip {
        let per = c.current_cpu_setup();
        g.irq_chips.0.insert(id, per);
    }
}
