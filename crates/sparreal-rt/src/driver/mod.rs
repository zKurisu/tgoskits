use alloc::{vec, vec::Vec};
use driver_interface::Register;

use crate::kernel;

mod uart;

pub fn registers() -> Vec<Register> {
    vec![uart::pl011::register()]
}
