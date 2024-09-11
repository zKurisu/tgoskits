use core::ptr::NonNull;

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use arm_pl011_rs::Pl011;
use driver_interface::*;
use embedded_io::*;
use future::LocalBoxFuture;
use futures::prelude::*;
use sparreal_kernel::driver::{self};

use crate::kernel;

struct RegisterPl011 {}

struct DriverPl011(Pl011);

unsafe impl Send for DriverPl011 {}
unsafe impl Sync for DriverPl011 {}

impl uart::Driver for DriverPl011 {}
impl io::Write for DriverPl011 {
    fn write(&mut self, buf: &[u8]) -> io::IOResult<usize> {
        match self.0.write(buf) {
            Ok(n) => Ok(n),
            Err(e) => Err(e.kind()),
        }
    }

    fn flush(&mut self) -> io::IOResult {
        Ok(())
    }
}

impl DriverGeneric for DriverPl011 {
    fn name(&self) -> String {
        "PL011".to_string()
    }
}

impl RegisterPl011 {
    async fn new_pl011(config: uart::Config) -> DriverResult<Box<dyn uart::Driver>> {
        let uart = Pl011::new(config.reg, None).await;
        Ok(Box::new(DriverPl011(uart)))
    }
}

impl uart::Register for RegisterPl011 {
    fn probe<'a>(
        &self,
        config: uart::Config,
    ) -> LocalBoxFuture<'a, DriverResult<Box<dyn uart::Driver>>> {
        Self::new_pl011(config).boxed_local()
    }
}

impl RegisterGeneric for RegisterPl011 {
    fn compatible(&self) -> Vec<String> {
        vec!["arm,pl011".to_string()]
    }
}

pub unsafe fn register_drivers() {
    kernel().module_driver().register_uart(RegisterPl011 {});
}
