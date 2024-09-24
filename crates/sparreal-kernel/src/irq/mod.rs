use crate::{driver::DriverLocked, sync::RwLock};

static IRQ_CHIP: RwLock<Option<DriverLocked>> = RwLock::new(None);

pub fn register_chip(chip: DriverLocked) {
    *IRQ_CHIP.write() = Some(chip);
}

fn get_chip() -> Option<DriverLocked> {
    IRQ_CHIP.read().clone()
}

