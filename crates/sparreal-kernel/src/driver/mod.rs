use crate::{globals::global_val, irq, platform, time};
use log::debug;
pub use rdrive::*;
pub use sparreal_macros::module_driver;

pub fn init() {
    let info = match &global_val().platform_info {
        crate::globals::PlatformInfoKind::DeviceTree(fdt) => DriverInfoKind::Fdt {
            addr: fdt.get_addr(),
        },
    };

    rdrive::init(info);

    rdrive::register_append(&platform::module_registers());

    debug!("add registers");

    rdrive::probe_with_kind(DriverKind::Intc).unwrap();

    irq::init_main_cpu();

    rdrive::probe_with_kind(DriverKind::Timer).unwrap();

    time::init_current_cpu();
}

pub fn probe() {
    rdrive::probe().unwrap();
}
