use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use driver_interface::*;
use futures::prelude::*;
use sparreal_kernel::driver::{self};

use crate::kernel;

struct RegisterPl011 {}

struct DriverPl011 {}

impl uart::Driver for DriverPl011 {}

impl DriverGeneric for DriverPl011 {
    fn name(&self) -> String {
        "PL011".to_string()
    }
}

impl uart::Register for RegisterPl011 {
    fn probe(&self, config: uart::Config) -> BoxFuture<DriverResult<Box<dyn uart::Driver>>> {
        async {
            let b: Box<dyn uart::Driver> = Box::new(DriverPl011 {});
            Ok(b)
        }
        .boxed()
    }
}

impl RegisterGeneric for RegisterPl011 {
    fn compatible(&self) -> Vec<String> {
        vec!["arm,pl011".to_string()]
    }
}

pub unsafe fn register_drivers() {
    kernel().driver_manager().register_uart(RegisterPl011 {});
}
