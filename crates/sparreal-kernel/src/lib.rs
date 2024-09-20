#![no_std]
#![feature(trait_upcasting)]

extern crate alloc;

mod kernel;

pub mod driver;
pub mod executor;
mod lang_items;
pub mod logger;
pub mod mem;
pub mod platform;
pub mod stdout;
pub mod sync;
pub mod time;
pub mod util;

pub use kernel::*;
pub use platform::Platform2;
pub use sparreal_macros::entry;
