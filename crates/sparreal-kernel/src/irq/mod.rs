use core::sync::atomic::{AtomicU64, Ordering};

use alloc::{boxed::Box, collections::btree_map::BTreeMap, format, vec::Vec};
use driver_interface::{
    IrqConfig,
    interrupt_controller::{self, InterruptControllerPerCpu},
};
use log::debug;

use crate::{driver_manager, globals, platform};

#[derive(Default)]
pub struct CpuIrqChips(BTreeMap<usize, Box<dyn interrupt_controller::InterruptControllerPerCpu>>);

pub(crate) fn init_current_cpu() {
    let chip = driver_manager::interrupt_controllers();
    let g = unsafe { globals::cpu_global_mut() };

    for (id, c) in chip {
        let per = c.current_cpu_setup();

        debug!("cpu {:#x} init irq", platform::cpu_id());
        g.irq_chips.0.insert(id, per);
    }
}

pub enum IrqHandle {
    Handled,
    None,
}

#[derive(Clone)]
pub struct IrqInfo {
    pub irq_chip_id: usize,
    pub cfg: IrqConfig,
}

fn chip(id: usize) -> &'static Box<dyn InterruptControllerPerCpu> {
    unsafe {
        globals::cpu_global_mut()
            .irq_chips
            .0
            .get(&id)
            .expect(format!("irq chip {:#x} not found", id).as_str())
    }
}

pub struct IrqRegister {
    pub info: IrqInfo,
    pub handler: Box<dyn Fn(usize) -> IrqHandle + 'static>,
    pub priority: Option<usize>,
    pub cpu_list: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqHandleId(u64);

impl IrqHandleId {
    fn new() -> Self {
        static ITER: AtomicU64 = AtomicU64::new(0);
        Self(ITER.fetch_add(1, Ordering::SeqCst))
    }
}

impl IrqRegister {
    pub fn register(self) -> IrqHandleId {
        let c = chip(self.info.irq_chip_id);
        let irq = self.info.cfg.irq;
        debug!("Enable irq {:?} on chip {:#x}", irq, self.info.irq_chip_id);
        let id = IrqHandleId::new();
        if let Some(p) = self.priority {
            c.set_priority(irq, p);
        } else {
            c.set_priority(irq, 0);
        }

        if !self.cpu_list.is_empty() {
            // c.set_bind_cpu(irq, &self.cpu_list);
        }

        c.set_trigger(irq, self.info.cfg.trigger);
        c.irq_enable(irq);
        id
    }
}
