#![no_std]

extern crate alloc;
extern crate sparreal_kernel;

#[cfg(feature = "rt")]
extern crate sparreal_rt;

use core::ptr::slice_from_raw_parts;

pub use bare_test_macros::tests;
pub use sparreal_kernel::globals::global_val;
pub use sparreal_kernel::platform::PlatformInfoKind;
pub use sparreal_kernel::prelude::*;

mod test_case;

#[sparreal_macros::entry]
fn main() -> ! {
    println!("begin test");

    for test in test_case_list() {
        println!("Run test: {}", test.name);

        (test.test_fn)();

        println!("test {} passed", test.name);
    }

    println!("All tests passed");
}

#[repr(C)]
#[derive(Clone)]
pub struct TestCase {
    pub name: &'static str,
    pub test_fn: fn(),
}

fn test_case_list() -> test_case::Iter<'static> {
    unsafe extern "C" {
        fn _stest_case();
        fn _etest_case();
    }

    let data = _stest_case as usize as *const u8;
    let len = _etest_case as usize - _stest_case as usize;

    let list = test_case::ListRef::from_raw(unsafe { &*slice_from_raw_parts(data, len) });

    list.iter()
}
