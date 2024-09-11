#![no_std]
#![no_main]

use core::ptr::NonNull;

use alloc::string::ToString;
use log::info;
use sparreal_rt::kernel;

extern crate alloc;
extern crate sparreal_rt;

#[sparreal_rt::entry]
fn main() {
    unsafe {
        info!("hello world");
        let s = "hello world".to_string();
        let st = s.as_str();
        // let a = *(dtb_addr as *const u8);
        // let b = a + 1;

        assert_eq!(st, "hello world");
    }
}
