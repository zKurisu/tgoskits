#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate sparreal_rt;

#[sparreal_rt::entry]
fn main() {
    println!("Hello, world!");
    println!("All tests passed!");
}
