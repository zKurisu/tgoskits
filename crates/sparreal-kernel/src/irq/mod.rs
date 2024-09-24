use crate::{
    driver::{DriverKind, DriverLocked},
    sync::RwLock,
};

static IRQ_CHIP: RwLock<Option<DriverLocked>> = RwLock::new(None);

pub fn register_chip(chip: DriverLocked) {
    *IRQ_CHIP.write() = Some(chip);
}

fn get_chip() -> Option<DriverLocked> {
    IRQ_CHIP.read().clone()
}

pub fn handle_irq() {
    if let Some(chip) = get_chip() {}
}

pub fn current_cpu_setup() {
    if let Some(chip) = get_chip() {
        let g = chip.write();
        if let DriverKind::Interupt(irq) = &g.kind {
            irq.current_cpu_setup();
        }
    }
}
