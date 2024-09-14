#![no_std]
#![feature(trait_upcasting)]

extern crate alloc;

mod kernel;

mod driver;
pub mod executor;
mod logger;
pub mod mem;
pub mod module;
pub mod platform;
pub mod stdout;
pub mod sync;
pub mod time;
pub mod util;

pub use kernel::*;
pub use platform::Platform;
pub use sparreal_macros::entry;
