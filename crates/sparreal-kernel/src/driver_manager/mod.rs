use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::ptr::NonNull;
use device::{BorrowGuard, Descriptor, Device, DriverId};
use log::info;

pub use driver_interface::{DriverRegister, ProbeFn};
use driver_interface::{RegAddress, interrupt_controller};
use fdt_parser::Fdt;
use spin::Mutex;

#[macro_use]
mod id;

pub mod device;
mod err;

pub use err::*;

static MANAGER: Mutex<Option<DeviceManager>> = Mutex::new(None);

#[derive(Default)]
struct DeviceManager {
    registers: Vec<DriverRegister>,
    irq_chip: BTreeMap<DriverId, Device<interrupt_controller::Driver>>,
}

pub fn init() {
    MANAGER.lock().replace(DeviceManager::default());
}

pub fn register_drivers(drivers: &[DriverRegister]) {
    MANAGER
        .lock()
        .as_mut()
        .unwrap()
        .registers
        .extend(drivers.iter().cloned());
}

fn registers() -> Vec<DriverRegister> {
    MANAGER.lock().as_ref().unwrap().registers.clone()
}

fn set_interrupt_controller(dev: Device<interrupt_controller::Driver>) {
    MANAGER
        .lock()
        .as_mut()
        .unwrap()
        .irq_chip
        .insert(dev.descriptor.driver_id, dev);
}

pub fn use_interrupt_controllers_by(who: &str) -> Vec<BorrowGuard<interrupt_controller::Driver>> {
    MANAGER
        .lock()
        .as_mut()
        .unwrap()
        .irq_chip
        .values()
        .map(|v| v.try_use_by(who))
        .filter_map(|d| match d {
            Ok(v) => Some(v),
            Err(_) => None,
        })
        .collect()
}

pub fn init_interrupt_controller_by_fdt(fdt_addr: NonNull<u8>) -> Result<(), String> {
    let fdt = Fdt::from_ptr(fdt_addr).map_err(|e| format!("{e:?}"))?;
    for r in registers() {
        if let ProbeFn::InterruptController(probe) = r.probe {
            let compa = r
                .compatibles
                .split("\n")
                .filter_map(|e| if e.is_empty() { None } else { Some(e) })
                .collect::<Vec<_>>();
            for node in fdt.find_compatible(&compa) {
                let reg = node
                    .reg()
                    .ok_or(format!("[{}] has no reg", node.name))?
                    .map(|reg| RegAddress {
                        addr: reg.address as _,
                        size: reg.size,
                    })
                    .collect();

                let mut irq = probe(reg);

                irq.open()?;

                let mut dev = Device::new(
                    Descriptor {
                        driver_id: node.phandle().unwrap().as_usize().into(),
                        name: node.name.to_string(),
                        ..Default::default()
                    },
                    irq,
                );

                info!("Driver add interrupt controller {:?}", dev);
                set_interrupt_controller(dev);
            }
        }
    }

    Ok(())
}
