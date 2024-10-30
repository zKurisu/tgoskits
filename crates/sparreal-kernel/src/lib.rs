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
pub use mem::mmu::iomap;
pub use mem::PhysAddr;
pub use platform::Platform;
pub use sparreal_macros::entry;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        $crate::stdout::print(format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n")
    };
    ($($arg:tt)*) => {{
        $crate::stdout::print(format_args!("{}\r\n", format_args!($($arg)*)));
    }};
}
