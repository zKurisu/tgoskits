#![no_std]

use alloc::{string::String, vec::Vec};

extern crate alloc;

pub use futures::future::BoxFuture;

pub mod io;
pub mod irq;
pub mod uart;

pub type DriverResult<T = ()> = Result<T, DriverError>;

pub trait DriverGeneric: Send + Sync + 'static {}

pub struct Register {
    pub name: String,
    pub compatible: Vec<&'static str>,
    pub kind: RegisterKind,
}

pub enum RegisterKind {
    Uart(uart::BoxRegister),
    Interupt(irq::BoxRegister),
    Spi,
}

impl Register {
    pub fn compatible_matched(&self, compatible: &str) -> bool {
        self.compatible.contains(&compatible)
    }
}

#[derive(Debug)]
pub enum DriverError {
    Init(String),
    NotFound,
    NoMemory,
}
