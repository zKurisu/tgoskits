pub use driver_interface::irq::*;
use driver_interface::RegisterKind;
use log::info;

use super::{irq_chip_list, probe_by_register, register_by_kind};

pub(super) async fn init_irq() {
    for reg in register_by_kind(RegisterKind::InteruptChip) {
        let _ = probe_by_register(reg).await;
    }

    for chip in irq_chip_list() {
        chip.driver.read().current_cpu_setup();
        info!("IRQ chip init success!");
    }
}
