#![no_std]
#![no_main]

extern crate somehal;

pub use sparreal_kernel::entry;
pub use sparreal_kernel::*;

mod hal_impl;

#[somehal::entry]
fn main() -> ! {
    somehal::println!("Starting Sparreal OS kernel...");
    let memory_map = somehal::mem::get_memory_map();
    sparreal_kernel::hal::setup::setup_allocator(memory_map);
    somehal::println!("Memory map set up.");
    somehal::post_allocator();
    sparreal_kernel::hal::setup::setup()
}
