#![no_std]
#![no_main]

extern crate somehal;

pub use somehal::*;
pub use sparreal_kernel::entry;
pub use sparreal_kernel::*;

#[somehal::entry]
fn main() -> ! {
    sparreal_kernel::hal::setup::setup_allocator();
    somehal::post_allocator();
    sparreal_kernel::hal::setup::setup()
}
