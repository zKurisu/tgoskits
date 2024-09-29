pub use driver_interface::irq::*;
use driver_interface::DriverKind;
use log::{debug, info};

use crate::{platform, struct_driver};

use super::{irq_chip_list, probe_by_register, register_by_kind, DriverCommon, DriverDescriptor};

pub(super) async fn init_irq() {
    for reg in register_by_kind(DriverKind::InteruptChip) {
        let _ = probe_by_register(reg).await;
    }

    for chip in irq_chip_list() {
        info!("CPU {} IRQ init", unsafe { platform::cpu_id_display() });
        chip.spec.read().current_cpu_setup();
        info!("IRQ chip init success!");
    }
}

struct_driver!(DriverIrqChip, BoxDriver);
