#![no_std]
#![feature(trait_upcasting)]

extern crate alloc;

mod kernel;

pub mod driver;
pub mod mem;
pub mod platform;
pub mod sync;
pub mod executor;
pub mod stdout;

pub use kernel::*;
use platform::app_main;
pub use platform::Platform;
pub use sparreal_macros::entry;
