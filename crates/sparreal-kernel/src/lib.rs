#![no_std]
#![feature(trait_upcasting)]

extern crate alloc;

pub mod kernel;

pub mod driver;
pub mod executor;
pub mod irq;
mod lang_items;
pub mod logger;
pub mod mem;
pub mod platform;
pub mod stdout;
pub mod sync;
pub mod time;
pub mod trap;
pub mod util;

pub use kernel::{KernelConfig, MemoryRange};
pub use platform::Platform;
pub use sparreal_macros::entry;
