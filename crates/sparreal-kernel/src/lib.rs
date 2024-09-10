#![no_std]

extern crate alloc;

mod kernel;

pub mod driver;
pub mod mem;
pub mod platform;
pub mod sync;

pub use kernel::*;
use platform::app_main;
pub use platform::Platform;
pub use sparreal_macros::entry;
