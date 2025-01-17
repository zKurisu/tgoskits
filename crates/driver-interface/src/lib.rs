#![cfg_attr(not(test), no_std)]

extern crate alloc;

use core::fmt::Display;

use alloc::string::String;

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

#[derive(Debug, Clone, Copy)]
pub struct RegAddress {
    pub addr: usize,
    pub size: Option<usize>,
}
