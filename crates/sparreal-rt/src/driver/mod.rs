use alloc::vec;

use crate::kernel;

mod uart;

pub fn register_all() {
    let registers = vec![uart::pl011::register()];

    kernel::get().module_driver().register_all(registers);
}
