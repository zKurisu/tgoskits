use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::*};
use driver_interface::{irq::IrqConfig, IrqProbeConfig};
use log::debug;

use crate::{
    driver::{irq_chip_list, DriverIrqChip},
    sync::RwLock,
};

type Handler = Box<dyn Fn(usize) -> IrqHandle + Send + Sync>;

static IRQ_VECTOR: RwLock<BTreeMap<usize, BTreeMap<String, Handler>>> =
    RwLock::new(BTreeMap::new());

pub enum IrqHandle {
    Handled,
    None,
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

fn get_chip() -> Option<DriverIrqChip> {
    irq_chip_list().first().cloned()
}

pub fn handle_irq() {
    if let Some(chip) = get_chip() {
        let c = chip.driver.read();

        let irq_id = c.get_and_acknowledge_interrupt();
        if let Some(irq_id) = irq_id {
            handle_irq_by_id(irq_id);
            c.end_interrupt(irq_id);
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
