use core::fmt::{self, Write};

use alloc::{boxed::Box, sync::Arc};
use driver_interface::uart;

use crate::{
    driver::{DriverArc, DriverWeak},
    platform,
    sync::RwLock,
    util::boot::boot_debug_hex,
};

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
pub struct UartWrite {
    pub driver: DriverWeak<uart::BoxDriver>,
}

impl UartWrite {
    pub fn new(driver: &DriverArc<uart::BoxDriver>) -> Self {
        Self {
            driver: Arc::downgrade(driver),
        }
    }
}

impl Write for UartWrite {
    fn write_str(&mut self, s: &str) -> Result<(), core::fmt::Error> {
        if let Some(arc) = self.driver.upgrade() {
            let mut g = arc.write();
            let _ = g.write_all(s.as_bytes());
        }
        Ok(())
    }
}
impl StdoutWrite for UartWrite {}

#[derive(Clone, Copy)]
pub struct EarlyDebugWrite;

impl Write for EarlyDebugWrite {
    fn write_str(&mut self, s: &str) -> Result<(), core::fmt::Error> {
        unsafe {
            s.bytes().for_each(|ch| {
                platform::debug_write_char(ch);
            });
        }
        Ok(())
    }
}
impl StdoutWrite for EarlyDebugWrite {}

pub fn early_print_str(s: &str) {
    let _ = EarlyDebugWrite {}.write_str(s);
}

#[macro_export]
macro_rules! dbg {
    ($v:expr) => {
        $crate::__export::early_print_str($v)
    };
}

#[macro_export]
macro_rules! dbgln {
    () => {
        $crate::dbg!("\r\n")
    };
    ($v:expr) => {
        $crate::dbg!($v);
        $crate::dbg!("\r\n")
    };
}

#[macro_export]
macro_rules! dbg_hex {
    ($v:expr) => {
        $crate::__export::debug_hex($v as _)
    };
}

#[macro_export]
macro_rules! dbg_hexln {
    ($v:expr) => {
        $crate::__export::debug_hex($v as _);
        $crate::dbgln!()
    };
}

pub fn debug_hex(v: u64) {
    boot_debug_hex(EarlyDebugWrite {}, v);
}
