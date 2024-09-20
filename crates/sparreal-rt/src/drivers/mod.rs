use alloc::{vec, vec::Vec};
use driver_interface::Register;

mod uart;

pub fn registers() -> Vec<Register> {
    vec![uart::pl011::register()]
}
