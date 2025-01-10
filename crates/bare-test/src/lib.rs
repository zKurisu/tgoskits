#![no_std]

extern crate alloc;
#[cfg(not(feature = "build"))]
extern crate sparreal_rt;

#[cfg(not(feature = "build"))]
pub use sparreal_rt::*;

pub use bare_test_macros::test_setup;

#[cfg(feature = "build")]
pub use sparreal_macros::build_test_setup;

#[cfg(not(feature = "build"))]
pub fn test_runner(tests: &[&dyn Fn()]) {
    pub use sparreal_rt::println;
    println!("Running {} tests", tests.len());
    for (i, test) in tests.iter().enumerate() {
        println!("[test {} start]", i);
        test();
        println!("[test {} passed]", i);
    }
    println!("All tests passed");
    shutdown()
}
