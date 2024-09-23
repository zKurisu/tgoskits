#![no_std]
#![feature(trait_upcasting)]

extern crate alloc;

pub mod kernel;

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
pub mod irq;

pub use kernel::{KernelConfig, MemoryRange};
pub use platform::Platform;
pub use sparreal_macros::entry;
