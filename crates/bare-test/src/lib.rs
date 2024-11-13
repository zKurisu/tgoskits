#![no_std]

extern crate alloc;
extern crate sparreal_rt;

pub use sparreal_rt::*;

pub fn test_runner(tests: &[&dyn Fn()]) {
    pub use sparreal_rt::println;
    println!("Running {} tests", tests.len());
    for (i, test) in tests.into_iter().enumerate() {
        println!("[test {} start]", i);
        test();
        println!("[test {} passed]", i);
    }
    println!("All tests passed");
    shutdown()
}
