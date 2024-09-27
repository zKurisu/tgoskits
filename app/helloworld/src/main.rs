#![no_std]
#![no_main]

use alloc::string::ToString;
use log::info;

extern crate alloc;
extern crate sparreal_rt;

#[sparreal_rt::entry]
fn main() {
    info!("hello world");
    let s = "hello world".to_string();
    let st = s.as_str();

    unsafe {
        // let a = *(0xffff_ffff_ffff_ffff as *const u8);
        // println!("{:x}", a);
    }

    assert_eq!(st, "hello world");
}
