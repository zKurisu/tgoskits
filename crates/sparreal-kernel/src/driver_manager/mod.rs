use alloc::{collections::BTreeMap, format, string::String, vec::Vec};
use core::ptr::NonNull;
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

                let irq = probe(reg);
                info!("Driver add interrupt controller [{}].", r.name);

                set_interrupt_controller(irq.into());
            }
        }
    }

    Ok(())
}
