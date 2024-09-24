use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::*, vec::Vec};
use driver_interface::irq::IrqConfig;
use log::debug;

use crate::{
    driver::{DriverKind, DriverLocked},
    sync::RwLock,
};

type Handler = Box<dyn Fn(usize) -> IrqHandle + Send + Sync>;

static IRQ_CHIP: RwLock<Option<DriverLocked>> = RwLock::new(None);
static IRQ_VECTOR: RwLock<BTreeMap<usize, BTreeMap<String, Handler>>> =
    RwLock::new(BTreeMap::new());

pub enum IrqHandle {
    Handled,
    None,
}

pub fn fdt_get_config(irqs: &[usize]) -> Option<IrqConfig> {
    if let Some(chip) = get_chip() {
        let g = chip.read();
        if let DriverKind::Interupt(irq) = &g.kind {
            return Some(irq.fdt_itr_to_config(irqs));
        }
    }
    None
}

pub fn register_irq<N, F>(irq_id: usize, dev_name: N, handler: F)
where
    N: ToString,
    F: Fn(usize) -> IrqHandle + Send + Sync + 'static,
{
    let mut vector = IRQ_VECTOR.write();
    let entry = vector.entry(irq_id).or_default();
    entry.insert(dev_name.to_string(), Box::new(handler));
}

pub fn register_chip(chip: DriverLocked) {
    *IRQ_CHIP.write() = Some(chip);
}

fn get_chip() -> Option<DriverLocked> {
    IRQ_CHIP.read().clone()
}

pub fn handle_irq() {
    if let Some(chip) = get_chip() {
        let g = chip.read();
        if let DriverKind::Interupt(irq) = &g.kind {
            let irq_id = irq.get_and_acknowledge_interrupt();
            if let Some(irq_id) = irq_id {
                handle_irq_by_id(irq_id);
                irq.end_interrupt(irq_id);
            }
        }
    }
}

fn handle_irq_by_id(irq_id: usize) {
    debug!("irq {}", irq_id);
    if let Some(handlers) = IRQ_VECTOR.read().get(&irq_id) {
        for (_, handler) in handlers {
            match handler(irq_id) {
                IrqHandle::Handled => {
                    break;
                }
                IrqHandle::None => {}
            }
        }
    }
}

pub fn current_cpu_setup() {
    if let Some(chip) = get_chip() {
        let g = chip.write();
        if let DriverKind::Interupt(irq) = &g.kind {
            irq.current_cpu_setup();
        }
    }
}
