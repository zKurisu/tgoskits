use alloc::{boxed::Box, collections::btree_map::BTreeMap, vec::Vec};
use driver_interface::{irq::Trigger, DriverId};
use log::{debug, info};

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

#[derive(Debug, Clone)]
pub struct IrqConfig {
    pub irq: usize,
    pub trigger: Trigger,
    pub priority: usize,
    pub cpu_list: Vec<u64>,
}

pub fn register_irq<F>(cfg: IrqConfig, dev_id: DriverId, handler: F)
where
    F: Fn(usize) -> IrqHandle + Send + Sync + 'static,
{
    let mut vector = IRQ_VECTOR.write();
    let entry = vector.entry(cfg.irq).or_default();
    entry.insert(dev_id, Box::new(handler));

    //TODO
    let controller_id = DriverId::default();

    if let Some(chip) = irq_chip_by_id_or_first(controller_id) {
        info!(
            "[{}]Enable irq {} on chip: {} ",
            dev_id, cfg.irq, chip.desc.name
        );
        let mut c = chip.spec.write();
        c.irq_enable(cfg.irq);
        c.set_priority(cfg.irq, cfg.priority);
        c.set_trigger(cfg.irq, cfg.trigger);
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
