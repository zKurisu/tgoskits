#![no_std]
#![no_main]
#![cfg(not(any(windows, unix)))]

extern crate alloc;
extern crate somehal;

pub use sparreal_kernel::entry;
pub use sparreal_kernel::*;

mod hal_impl;

#[somehal::entry]
fn main() -> ! {
    somehal::print!("{LOGO}");
    sparreal_kernel::hal::setup::start_kernel()
}

const LOGO: &str = "
\x1b[38;2;255;255;255m      🐦 SparrealOS 🐦
\x1b[0m
";
