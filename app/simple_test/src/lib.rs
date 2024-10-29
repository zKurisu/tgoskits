#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(bare_test::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

#[bare_test::entry]
fn main() {
    #[cfg(test)]
    test_main();

    loop {}
}

#[cfg(test)]
mod test2 {
    extern crate bare_test;

    use bare_test::println;
    use log::info;

    #[test_case]
    fn trivial_assertion() {
        println!("hello world from test!");
        info!("trivial assertion... ");
        assert_eq!(1, 1);
    }
}
