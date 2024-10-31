use super::{
    device_id_by_node_name, device_tree::get_device_tree, DeviceId, DriverIrqChip, DriverTimer,
    DriverUart,
};

use crate::{driver::device_tree::FDTExtend as _, sync::RwLock};
use alloc::{
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    string::{String, ToString},
    vec::Vec,
};
use driver_interface::{DriverKind, DriverSpecific, ProbeConfig, Register};
use log::{error, info};

pub(super) static CONTAINER: Container = Container::new();

type ContainerKind<T> = RwLock<BTreeMap<DeviceId, T>>;

const fn new_kind<T>() -> ContainerKind<T> {
    return RwLock::new(BTreeMap::new());
}

pub(super) struct Container {
    pub(super) registers: RwLock<BTreeMap<String, Register>>,
    probed: RwLock<BTreeSet<DeviceId>>,
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

    pub fn add_driver(&self, id: DeviceId, name: String, spec: DriverSpecific) {
        macro_rules! add_to {
            ($driver:expr,$field:ident) => {
                let d = $crate::driver::DriverCommon::new(id.clone(), name, $driver);
                self.$field.write().insert(id, d.into());
            };
        }

        match spec {
            DriverSpecific::Uart(driver) => {
                add_to!(driver, uart);
            }
            DriverSpecific::InteruptChip(driver) => {
                add_to!(driver, irq_chip);
            }
            DriverSpecific::Timer(driver) => {
                add_to!(driver, timer);
            }
        }
    }
}
pub fn add_driver<N: ToString>(id: DeviceId, name: N, spec: DriverSpecific) {
    CONTAINER.add_driver(id, name.to_string(), spec);
}

pub async fn probe_by_register(register: Register) -> Option<()> {
    let fdt = get_device_tree()?;
    let node = fdt.find_compatible(&register.compatible).next()?;

    let config = node.probe_config();

    probe(config, register).await;
    Some(())
}

pub async fn probe_by_node(node: fdt_parser::Node<'_>) -> Option<()> {
    let id = device_id_by_node_name(node.name);

    if is_probed(&id) {
        return Some(());
    }

    let caps = node.compatibles().collect::<Vec<_>>();
    let register = register_by_compatible(&caps)?;
    let config = node.probe_config();

    probe(config, register).await;
    Some(())
}

pub(crate) fn is_probed(id: &DeviceId) -> bool {
    CONTAINER.probed.read().contains(id)
}

pub async fn probe(config: ProbeConfig, register: Register) -> Option<()> {
    let id = config.id;
    if is_probed(&id) {
        return None;
    }
    info!("[{}]Probe driver [{}]", id, register.name);
    for irq in &config.irq {
        info!("[{}]    Irq: {}, triger {:?}", id, irq.irq, irq.trigger);
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

pub fn uart_by_id(id: DeviceId) -> Option<DriverUart> {
    CONTAINER.uart.read().get(&id).cloned()
}

pub fn irq_chip_list() -> Vec<DriverIrqChip> {
    let g = CONTAINER.irq_chip.read();
    g.values().cloned().collect()
}
pub fn irq_chip_by_id_or_first(id: DeviceId) -> Option<DriverIrqChip> {
    let g = CONTAINER.irq_chip.read();
    g.get(&id).or_else(|| g.values().next()).cloned()
}

pub fn irq_by_id(id: DeviceId) -> Option<DriverIrqChip> {
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
