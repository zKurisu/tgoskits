#![no_std]
#![no_main]
extern crate alloc;

use log::info;
use sparreal_rt::prelude::*;

#[entry]
fn main() {
    info!("Hello, world!");
}
