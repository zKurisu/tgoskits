use alloc::{
    string::{String, ToString},
    vec::Vec,
};
pub use driver_interface::irq::*;
use driver_interface::{Register, RegisterKind};
use log::{error, info};

use crate::mem::mmu::iomap;

use super::{device_tree::get_device_tree, manager::Manager, DriverKind};

impl Manager {
    pub(super) async fn init_irq(&mut self) {
        let mut found = None;
        for register in self.registers.values() {
            if let Some(res) = probe_irq(register).await {
                found = Some(res);
                break;
            }
        }

        if let Some((name, driver)) = found {
            let chip = self.add_driver(name, DriverKind::Interupt(driver));
            crate::irq::register_chip(chip);
        }
    }
}

async fn probe_irq(register: &Register) -> Option<(String, BoxDriver)> {
    let fdt = get_device_tree()?;
    if let RegisterKind::Interupt(ref kind) = register.kind {
        let node = fdt.find_compatible(&register.compatible)?;

        let mut regs = Vec::new();

        for reg in node.reg_fix() {
            let reg_base = iomap(reg.starting_address.into(), reg.size.unwrap_or(0x1000));
            regs.push(reg_base);
        }

        if regs.len() < 2 {
            error!("irq node {} has less than 2 regs", node.name);
            return None;
        }
        info!(
            "Probe {} - irq: {}",
            node.name,
            node.compatible()
                .and_then(|cap| cap.first())
                .unwrap_or_default()
        );
        let driver = kind
            .probe(Config {
                reg1: regs[0],
                reg2: regs[1],
            })
            .await
            .inspect_err(|e| error!("Probe irq failed: {:?}", e))
            .ok()?;
        info!("    probe irq success");
        return Some((node.name.to_string(), driver));
    }

    None
}
