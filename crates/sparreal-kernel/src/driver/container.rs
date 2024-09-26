use super::{
    device_tree::get_device_tree, driver_id_by_node_name, DriverArc, DriverCommon,
    DriverDescriptor, DriverId, DriverIrqChip, DriverTimer, DriverUart,
};

use crate::{driver::device_tree::FDTExtend as _, sync::RwLock};
use alloc::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use driver_interface::{irq, uart, DriverKind, DriverSpecific, ProbeConfig, Register};
use flat_device_tree::node::FdtNode;
use log::{error, info};

pub(super) static CONTAINER: Container = Container::new();

type ContainerKind<T> = RwLock<BTreeMap<DriverId, T>>;

const fn new_kind<T>() -> ContainerKind<T> {
    return RwLock::new(BTreeMap::new());
}

pub(super) struct Container {
    pub(super) registers: RwLock<BTreeMap<String, Register>>,
    probed: RwLock<BTreeSet<DriverId>>,
    pub(super) uart: ContainerKind<DriverUart>,
    pub(super) irq_chip: ContainerKind<DriverIrqChip>,
    pub(super) timer: ContainerKind<DriverTimer>,
}

impl Container {
    const fn new() -> Self {
        Self {
            registers: RwLock::new(BTreeMap::new()),
            probed: RwLock::new(BTreeSet::new()),
            uart: new_kind(),
            irq_chip: new_kind(),
            timer: new_kind(),
        }
    }
}
pub fn add_driver<N: ToString>(id: DriverId, name: N, spec: DriverSpecific) {
    macro_rules! add_to {
        ($driver:expr,$field:expr) => {
            let d = DriverCommon::new(id.clone(), name, $driver);
            $field.write().insert(id, d.into());
        };
    }

    match spec {
        DriverSpecific::Uart(driver) => {
            add_to!(driver, CONTAINER.uart);
        }
        DriverSpecific::InteruptChip(driver) => {
            add_to!(driver, CONTAINER.irq_chip);
        }
        DriverSpecific::Timer(driver) => {
            add_to!(driver, CONTAINER.timer);
        }
    }
}

pub async fn probe_by_register(register: Register) -> Option<()> {
    let fdt = get_device_tree()?;
    let node = fdt.find_compatible(&register.compatible)?;

    let config = node.probe_config();

    probe(config, register).await;
    Some(())
}

pub async fn probe_by_node(node: FdtNode<'_, '_>) -> Option<()> {
    let id = driver_id_by_node_name(node.name);

    if is_probed(&id) {
        return Some(());
    }

    let caps = node.compatible()?.all().collect::<Vec<_>>();
    let register = register_by_compatible(&caps)?;
    let config = node.probe_config();

    probe(config, register).await;
    Some(())
}

pub(crate) fn is_probed(id: &DriverId) -> bool {
    CONTAINER.probed.read().contains(id)
}

pub async fn probe(config: ProbeConfig, register: Register) -> Option<()> {
    let id = config.id;
    if is_probed(&id) {
        return None;
    }
    info!("[{}]Probe driver [{}]", id, register.name);
    for irq in &config.irq {
        info!("[{}]    Irq: {}, triger {:?}", id, irq.irq_id, irq.trigger);
    }

    let kind = register
        .probe
        .probe(config)
        .await
        .inspect_err(|e| error!("{:?}", e))
        .ok()?;
    info!("[{}]Probe success!", id);

    CONTAINER.probed.write().insert(id);

    add_driver(id, register.name, kind);
    Some(())
}

pub fn uart_list() -> Vec<DriverUart> {
    let g = CONTAINER.uart.read();
    g.values().cloned().collect()
}

pub fn uart_by_id(id: DriverId) -> Option<DriverUart> {
    CONTAINER.uart.read().get(&id).cloned()
}

pub fn irq_chip_list() -> Vec<DriverIrqChip> {
    let g = CONTAINER.irq_chip.read();
    g.values().cloned().collect()
}
pub fn irq_chip_by_id_or_first(id: DriverId) -> Option<DriverIrqChip> {
    let g = CONTAINER.irq_chip.read();
    g.get(&id).or_else(|| g.values().next()).cloned()
}

pub fn irq_by_id(id: DriverId) -> Option<DriverIrqChip> {
    CONTAINER.irq_chip.read().get(&id).cloned()
}

pub fn register_append(registers: impl IntoIterator<Item = Register>) {
    let mut c = CONTAINER.registers.write();
    for reg in registers {
        for cap in &reg.compatible {
            c.insert(cap.to_string(), reg.clone());
        }
    }
}

pub fn register_by_compatible(compatible: &[&str]) -> Option<Register> {
    let c = CONTAINER.registers.read();
    for cap in compatible {
        if let Some(reg) = c.get(*cap) {
            return Some(reg.clone());
        }
    }
    None
}

pub fn register_by_kind(kind: DriverKind) -> Vec<Register> {
    let c = CONTAINER.registers.read();
    c.values().filter(|one| one.kind == kind).cloned().collect()
}

pub fn register_all() -> Vec<Register> {
    let c = CONTAINER.registers.read();
    c.values().cloned().collect()
}
