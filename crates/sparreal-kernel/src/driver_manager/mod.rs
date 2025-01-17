use alloc::{string::String, vec::Vec};
use core::ptr::NonNull;
use device::{
    BorrowGuard,
    irq::{self},
};
use log::warn;

pub use driver_interface::DriverRegister;
use driver_interface::interrupt_controller;
use spin::{Mutex, MutexGuard};

#[macro_use]
mod id;

pub mod device;
mod err;

pub use err::*;

static MANAGER: Mutex<DeviceManager> = Mutex::new(DeviceManager::new());

pub struct DeviceManager {
    registers: Vec<DriverRegister>,
    pub irq_chip: device::irq::Container,
}

impl DeviceManager {
    const fn new() -> Self {
        Self {
            registers: Vec::new(),
            irq_chip: device::irq::Container::new(),
        }
    }
}

pub fn init_irq_chips_by_fdt(fdt_addr: NonNull<u8>) -> Result<(), String> {
    let dev_list = irq::init_by_fdt(&registers(), fdt_addr)?;
    manager().irq_chip.set_list(dev_list);
    Ok(())
}

pub fn manager() -> MutexGuard<'static, DeviceManager> {
    MANAGER.lock()
}

pub fn register_drivers(drivers: &[DriverRegister]) {
    MANAGER.lock().registers.extend(drivers.iter().cloned());
}

fn registers() -> Vec<DriverRegister> {
    MANAGER.lock().registers.clone()
}

pub fn use_irq_chips_by(who: &str) -> Vec<BorrowGuard<interrupt_controller::Driver>> {
    MANAGER
        .lock()
        .irq_chip
        .irq_chip_list()
        .into_iter()
        .map(|v| v.try_use_by(who))
        .filter_map(|d| match d {
            Ok(v) => Some(v),
            Err(e) => {
                warn!("{}", e);
                None
            }
        })
        .collect()
}
