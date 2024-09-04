#![no_std]
#![no_main]

use core::ptr::NonNull;

use sparreal_rt::kernel;

extern crate alloc;
extern crate sparreal_rt;

#[sparreal_rt::entry]
fn main() {

    extern "C" {
        fn _stack_top();
    }
    unsafe {
        let heap_start = NonNull::new_unchecked(_stack_top as *mut u8);

        // let a = *(dtb_addr as *const u8);
        // let b = a + 1;

        assert_eq!(heap_start.read(), 1);
    }
}
