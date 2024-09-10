use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use sparreal_kernel::driver::{
    self,
    manager::{Driver, DriverRegister, DriverRegisterUart},
};

use crate::kernel;

struct RegisterPl011 {}

struct DriverPl011 {}

impl driver::manager::DriverUart for DriverPl011 {}

impl Driver for DriverPl011 {}

impl DriverRegisterUart for RegisterPl011 {
    fn probe(&self) -> alloc::boxed::Box<dyn sparreal_kernel::driver::manager::DriverUart> {
        Box::new(DriverPl011 {})
    }
}

impl DriverRegister for RegisterPl011 {
    fn compatible(&self) -> Vec<String> {
        vec!["arm,pl011".to_string()]
    }
}

pub unsafe fn register_drivers() {
    kernel().driver.register_uart(RegisterPl011 {});
}
