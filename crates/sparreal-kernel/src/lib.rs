#![no_std]

mod kernel;
pub mod mem;
pub mod platform;

pub use kernel::*;
pub use platform::Platform;
use platform::app_main;
pub use sparreal_macros::entry;

