use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};

use sparreal_kernel::driver::manager::{Driver, DriverRegisterUart};

use crate::kernel;

struct RegisterPl011 {}

impl DriverRegisterUart for RegisterPl011 {}

impl Driver for RegisterPl011 {
    fn compatible(&self) -> Vec<String> {
        vec!["arm-pl011".to_string()]
    }
}

pub unsafe fn register_drivers() {
    kernel().driver.register_uart(RegisterPl011 {});
}
