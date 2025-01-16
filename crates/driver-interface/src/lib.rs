#![cfg_attr(not(test), no_std)]

extern crate alloc;

use core::fmt::Display;

use alloc::string::String;
pub use interrupt_controller::IrqConfig;

pub(crate) mod _macro;
pub mod interrupt_controller;
mod register;
pub use register::*;
pub(crate) mod err;
pub use err::DriverError;
pub use err::DriverResult;

pub trait DriverGeneric: Send {
    fn name(&self) -> String;
    fn open(&mut self) -> Result<(), String>;
}

#[repr(transparent)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceId(u64);

impl Into<u64> for DeviceId {
    fn into(self) -> u64 {
        self.0
    }
}

impl From<u64> for DeviceId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Display for DeviceId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RegAddress {
    pub addr: usize,
    pub size: Option<usize>,
}
