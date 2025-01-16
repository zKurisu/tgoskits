use alloc::{collections::BTreeMap, format, string::String, vec::Vec};
use core::ptr::NonNull;
use driver::BorrowGuard;
use log::info;

use driver_interface::RegAddress;
pub use driver_interface::{DriverRegister, ProbeFn};
use fdt_parser::Fdt;
use spin::Mutex;

pub mod driver;

static MANAGER: Mutex<Option<DeviceManager>> = Mutex::new(None);

#[derive(Default)]
struct DeviceManager {
    registers: Vec<DriverRegister>,
    irq_chip: BTreeMap<usize, driver::interrupt_controller::Device>,
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

fn set_interrupt_controller(dev: driver::interrupt_controller::Device) {
    MANAGER
        .lock()
        .as_mut()
        .unwrap()
        .irq_chip
        .insert(dev.id as _, dev);
}

pub fn interrupt_controllers() -> Vec<(
    usize,
    BorrowGuard<driver_interface::interrupt_controller::BoxedDriver>,
)> {
    MANAGER
        .lock()
        .as_mut()
        .unwrap()
        .irq_chip
        .iter()
        .map(|v| (v.0, v.1.driver.get()))
        .filter_map(|(id, d)| match d {
            Ok(v) => Some((*id, v)),
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

                let mut dev: driver::interrupt_controller::Device = irq.into();
                dev.id = node.phandle().unwrap().as_usize() as _;

                info!(
                    "Driver add interrupt controller [{:#x}] [{}].",
                    dev.id, r.name
                );
                set_interrupt_controller(dev);
            }
        }
    }

    Ok(())
}
