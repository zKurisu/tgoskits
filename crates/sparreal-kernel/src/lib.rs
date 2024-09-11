#![no_std]
#![feature(trait_upcasting)]

extern crate alloc;

mod kernel;

pub mod driver;
pub mod executor;
mod logger;
pub mod mem;
pub mod module;
pub mod platform;
pub mod stdout;
pub mod sync;
pub mod time;

pub use kernel::*;
use platform::app_main;
pub use platform::Platform;
pub use sparreal_macros::entry;

