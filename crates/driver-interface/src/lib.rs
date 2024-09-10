#![no_std]

use alloc::{boxed::Box, string::String, vec::Vec};

extern crate alloc;

pub use futures::future::BoxFuture;

pub mod io;
pub mod uart;

pub type DriverResult<T = ()> = Result<T, DriverError>;

pub trait DriverGeneric {
    fn name(&self) -> String;
}

pub trait RegisterGeneric {
    fn compatible(&self) -> Vec<String>;

    fn compatible_matched(&self, compatible: &str) -> bool {
        for one in self.compatible() {
            if one.as_str().eq(compatible) {
                return true;
            }
        }
        false
    }
}

#[derive(Debug)]
pub enum DriverError {
    NotFound,
    NoMemory,
}
