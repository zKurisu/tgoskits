#![no_std]
#![no_main]
extern crate alloc;

use core::{hint::spin_loop, time::Duration};

use log::info;
use sparreal_kernel::{platform::shutdown, time::{self, spin_delay}};
use sparreal_rt::prelude::*;

#[entry]
fn main() {
    info!("Hello, world!");

    time::after(Duration::from_secs(1), || {
        info!("Timer callback");
        // shutdown();
    });

    loop {
        spin_delay(Duration::from_secs(1));
        info!("123");
    }
}
