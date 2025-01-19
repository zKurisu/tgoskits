#![no_std]
#![no_main]
extern crate alloc;

use core::time::Duration;

use alloc::string::ToString;
use log::info;
use sparreal_kernel::{
    platform::shutdown,
    task::{self, TaskConfig},
    time::{self, spin_delay},
};
use sparreal_rt::prelude::*;

#[entry]
fn main() {
    info!("Hello, world!");

    time::after(Duration::from_secs(1), || {
        info!("Timer callback");
        // shutdown();
    });

    task::spawn_with_config(
        || {
            info!("task2");
        },
        TaskConfig {
            name: "task2".to_string(),
            priority: 0,
            stack_size: 0x1000 * 4,
        },
    )
    .unwrap();


    

    loop {
        spin_delay(Duration::from_secs(1));
        info!("123");
    }
}
