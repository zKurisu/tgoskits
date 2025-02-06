#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub(crate) mod _macro;
pub mod interrupt_controller;
mod register;
pub mod timer;
pub use register::*;
pub(crate) mod err;
pub use err::{DriverError, DriverResult};
pub use interrupt_controller::IrqConfig;

pub trait DriverGeneric: Send {
    fn open(&mut self) -> DriverResult;
    fn close(&mut self) -> DriverResult;
}

#[derive(Debug, Clone, Copy)]
pub struct RegAddress {
    pub addr: usize,
    pub size: Option<usize>,
}
