#![no_std]
#![feature(linkage)]
#![feature(fn_align)]

extern crate alloc;

pub mod __export;
pub mod boot;
mod driver_manager;
pub mod globals;
pub mod io;

pub mod async_std;
pub mod irq;
mod lang_items;
mod logger;
pub mod mem;
pub mod platform;
pub mod platform_if;
pub mod prelude;
pub mod task;
pub mod time;

pub use driver_interface;
pub use mem::Address;
