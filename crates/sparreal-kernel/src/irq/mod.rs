use alloc::{collections::btree_map::BTreeMap, sync::Arc, vec::Vec};
use driver_interface::{irq::Trigger, DeviceId};
use log::{debug, info, warn};

use crate::{
    driver::{irq_chip_by_id_or_first, irq_chip_list, DriverIrqChip},
    platform,
    sync::RwLock,
};

type Handler = Arc<dyn Fn(usize) -> IrqHandle>;

static IRQ_VECTOR: RwLock<Vector> = RwLock::new(Vector::new());

struct VectorPerIrq(BTreeMap<DeviceId, Handler>);
struct VectorPerCpu(BTreeMap<usize, VectorPerIrq>);
struct Vector(BTreeMap<usize, VectorPerCpu>);

unsafe impl Send for Vector {}
unsafe impl Sync for Vector {}

#[allow(unused)]
impl Vector {
    const fn new() -> Self {
        Self(BTreeMap::new())
    }

    fn insert(&mut self, cpu_id: usize, irq_num: usize, dev_id: DeviceId, handler: Handler) {
        self.0
            .entry(cpu_id)
            .or_insert(VectorPerCpu(BTreeMap::new()))
            .0
            .entry(irq_num)
            .or_insert(VectorPerIrq(BTreeMap::new()))
            .0
            .insert(dev_id, handler);
    }

    fn remove(&mut self, cpu_id: usize, irq_num: usize, dev_id: DeviceId) {
        self.0
            .get_mut(&cpu_id)
            .and_then(|one| one.0.get_mut(&irq_num))
            .and_then(|one| one.0.remove(&dev_id));
    }

    fn get_handle_stack(&self, irq_num: usize) -> Vec<Handler> {
        let cpu_id = unsafe { platform::cpu_id() } as usize;
        self.0
            .get(&cpu_id)
            .and_then(|map| map.0.get(&irq_num))
            .map(|map| map.0.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    }
}

fn vector_cpu_set_handle(cpu_id: usize, irq_num: usize, dev_id: DeviceId, handler: Handler) {
    IRQ_VECTOR.write().insert(cpu_id, irq_num, dev_id, handler);
}
pub fn irq_set_handle(
    irq_num: usize,
    dev_id: DeviceId,
    handler: impl Fn(usize) -> IrqHandle + 'static,
) {
    let handler = Arc::new(handler);
    vector_cpu_set_handle(unsafe { platform::cpu_id() } as _, irq_num, dev_id, handler);
}

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
pub fn irq_setup(irq: usize, dev_id: DeviceId, trigger: Trigger) {
    //TODO
    let controller_id = DeviceId::default();

    if let Some(chip) = irq_chip_by_id_or_first(controller_id) {
        info!(
            "[{}]Enable irq {} on chip: {} ",
            dev_id, irq, chip.desc.name
        );
        let mut c = chip.spec.write();
        c.irq_enable(irq);
        c.set_priority(irq, 0);
        c.set_trigger(irq, trigger);
    }
}

pub fn register_irq(
    cfg: IrqConfig,
    dev_id: DeviceId,
    handler: impl Fn(usize) -> IrqHandle + 'static,
) {
    irq_set_handle(cfg.irq, dev_id, handler);

    //TODO
    let controller_id = DeviceId::default();

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
    let stack = { IRQ_VECTOR.read().get_handle_stack(irq_id) };

    for handler in stack {
        match handler(irq_id) {
            IrqHandle::Handled => {
                return;
            }
            IrqHandle::None => {}
        }
    }

    warn!("Irq {} not handled", irq_id);
}
