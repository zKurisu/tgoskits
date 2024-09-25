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
    // let a = *(dtb_addr as *const u8);
    // let b = a + 1;

    assert_eq!(st, "hello world");
}
