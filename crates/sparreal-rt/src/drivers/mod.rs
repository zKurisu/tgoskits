use alloc::{vec, vec::Vec};
use driver_interface::Register;

mod gic;
mod timer;
mod uart;

pub fn registers() -> Vec<Register> {
    vec![
        gic::register_v2(),
        gic::register_v3(),
        timer::armv8::register(),
        uart::pl011::register(),
    ]
}
