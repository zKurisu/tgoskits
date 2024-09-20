use core::fmt::{self, Write};

use alloc::{boxed::Box, string::String, sync::Arc};
use driver_interface::io::*;

use crate::{driver::DriverKind, driver_manager, platform, sync::RwLock};

static STDOUT: RwLock<Option<Box<dyn StdoutWrite>>> = RwLock::new(None);

pub trait StdoutWrite: Send + Sync + Write + 'static {}

pub fn print(args: fmt::Arguments) {
    let mut stdout = STDOUT.write();
    if let Some(stdout) = stdout.as_mut() {
        let _ = stdout.write_fmt(args);
    }
}

pub fn set_stdout(stdout: impl StdoutWrite) {
    let stdout = Box::new(stdout);
    *STDOUT.write() = Some(stdout);
}

#[derive(Clone)]
pub struct DriverWrite {
    pub name: String,
}

impl DriverWrite {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl Write for DriverWrite {
    fn write_str(&mut self, s: &str) -> Result<(), core::fmt::Error> {
        if let Some(driver) = driver_manager().get_driver(&self.name) {
            let mut g = driver.write();
            if let DriverKind::Uart(uart) = &mut g.kind {
                let _ = uart.write_all(s.as_bytes());
            }
        }
        Ok(())
    }
}
impl StdoutWrite for DriverWrite {}

#[derive(Clone, Copy)]
pub struct EarlyDebugWrite;

impl Write for EarlyDebugWrite {
    fn write_str(&mut self, s: &str) -> Result<(), core::fmt::Error> {
        unsafe {
            s.chars().for_each(|ch| {
                platform::debug_write_char(ch);
            });
        }
        Ok(())
    }
}
impl StdoutWrite for EarlyDebugWrite {}
