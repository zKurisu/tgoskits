use super::{
    device_tree::get_device_tree, DriverArc, DriverId, DriverInfo, DriverIrqChip, DriverUart,
};

use crate::{driver::device_tree::FDTExtend as _, sync::RwLock};
use alloc::{
    collections::btree_map::BTreeMap,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use driver_interface::{irq, uart, DriverKind, Register, RegisterKind};
use log::{error, info};

pub(super) static CONTAINER: Container = Container::new();

type ContainerKind<T> = RwLock<BTreeMap<DriverId, T>>;

const fn new_kind<T>() -> ContainerKind<T> {
    return RwLock::new(BTreeMap::new());
}

fn new_driver_locked<N: ToString, T>(id: DriverId, name: N, kind: T) -> (DriverInfo, DriverArc<T>) {
    (
        DriverInfo {
            id,
            name: name.to_string(),
        },
        Arc::new(RwLock::new(kind)),
    )
}

pub(super) struct Container {
    pub(super) registers: RwLock<BTreeMap<String, Register>>,
    pub(super) info: ContainerKind<DriverInfo>,
    pub(super) uart: ContainerKind<DriverArc<uart::BoxDriver>>,
    pub(super) irq_chip: ContainerKind<DriverArc<irq::BoxDriver>>,
}

impl Container {
    const fn new() -> Self {
        Self {
            registers: RwLock::new(BTreeMap::new()),
            uart: new_kind(),
            irq_chip: new_kind(),
            info: new_kind(),
        }
    }
}
pub fn add_driver<N: ToString>(id: DriverId, name: N, kind: DriverKind) {
    macro_rules! add_to {
        ($driver:expr, $field:expr) => {
            let (info, driver) = new_driver_locked(id.clone(), name.to_string(), $driver);
            CONTAINER.info.write().insert(id.clone(), info);
            $field.write().insert(id, driver);
        };
    }

    match kind {
        DriverKind::Uart(driver) => {
            add_to!(driver, CONTAINER.uart);
        }
        DriverKind::InteruptChip(driver) => {
            add_to!(driver, CONTAINER.irq_chip);
        }
    }
}

pub async fn probe_by_register(reg: Register) -> Option<()> {
    let fdt = get_device_tree()?;
    let node = fdt.find_compatible(&reg.compatible)?;

    if CONTAINER.info.read().contains_key(&node.name.into()) {
        return None;
    }

    let config = node.probe_config();

    info!("Probe node [{}], driver [{}]", node.name, reg.name);
    for irq in &config.irq {
        info!("    Irq: {}, triger {:?}", irq.irq_id, irq.trigger);
    }

    let kind = reg
        .probe
        .probe(config)
        .await
        .inspect_err(|e| error!("{:?}", e))
        .ok()?;
    info!("Probe success!");

    add_driver(node.name.into(), reg.name, kind);
    Some(())
}

pub fn uart_list() -> Vec<DriverUart> {
    let g = CONTAINER.uart.read();
    g.iter()
        .map(|(k, v)| {
            let info = get_info(k).unwrap();
            DriverUart {
                info,
                driver: v.clone(),
            }
        })
        .collect()
}

pub fn uart_by_id(id: DriverId) -> Option<DriverArc<uart::BoxDriver>> {
    CONTAINER.uart.read().get(&id).cloned()
}

pub fn irq_chip_list() -> Vec<DriverIrqChip> {
    let g = CONTAINER.irq_chip.read();
    g.iter()
        .map(|(k, v)| {
            let info = get_info(k).unwrap();
            DriverIrqChip {
                info,
                driver: v.clone(),
            }
        })
        .collect()
}

pub fn irq_by_id(id: DriverId) -> Option<DriverArc<irq::BoxDriver>> {
    CONTAINER.irq_chip.read().get(&id).cloned()
}

fn get_info(id: &DriverId) -> Option<DriverInfo> {
    CONTAINER.info.read().get(id).cloned()
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

pub fn register_by_kind(kind: RegisterKind) -> Vec<Register> {
    let c = CONTAINER.registers.read();
    c.values().filter(|one| one.kind == kind).cloned().collect()
}
