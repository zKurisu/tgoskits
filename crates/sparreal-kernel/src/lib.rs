#![no_std]

#[allow(unused_imports)]
#[macro_use]
extern crate alloc;
#[macro_use]
extern crate log;

pub mod __export;
pub mod hal;
mod lang;
pub mod os;

pub use sparreal_macros::entry;
