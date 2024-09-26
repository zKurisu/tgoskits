use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::*};
use driver_interface::{irq::IrqConfig, DriverId, IrqProbeConfig};
use log::debug;

use crate::{
    driver::{irq_chip_by_id_or_first, irq_chip_list, DriverIrqChip},
    sync::RwLock,
};

type Handler = Box<dyn Fn(usize) -> IrqHandle + Send + Sync>;

static IRQ_VECTOR: RwLock<BTreeMap<usize, BTreeMap<DriverId, Handler>>> =
    RwLock::new(BTreeMap::new());

pub enum IrqHandle {
    Handled,
    None,
}

pub fn register_irq<F>(irq: IrqConfig, dev_id: DriverId, handler: F)
where
    F: Fn(usize) -> IrqHandle + Send + Sync + 'static,
{
    let mut vector = IRQ_VECTOR.write();
    let entry = vector.entry(irq.irq_id).or_default();
    entry.insert(dev_id, Box::new(handler));

    //TODO
    let controller_id = DriverId::default();

    if let Some(chip) = irq_chip_by_id_or_first(controller_id) {
        debug!("irq chip: {}", chip.desc.name);
        chip.spec.write().enable_irq(irq);
    }
}

fn get_chip() -> Option<DriverIrqChip> {
    irq_chip_list().first().cloned()
}

pub fn handle_irq() {
    if let Some(chip) = get_chip() {
        let c = chip.spec.read();

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
