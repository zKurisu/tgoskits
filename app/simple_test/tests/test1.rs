#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(bare_test::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate bare_test;

#[bare_test::entry]
fn main() {
    test_main();
}

use bare_test::println;
#[test_case]
fn it_works2() {
    println!("test2... ");
    assert_eq!(1, 2);
}
#[test_case]
fn it_works1() {
    println!("test1... ");
    assert_eq!(1, 1);
}
