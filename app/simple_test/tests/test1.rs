#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(bare_test::test_runner)]
#![reexport_test_harness_main = "test_main"]

bare_test::test_setup!();

use bare_test::{driver::device_tree::get_device_tree, mem::mmu::iomap, println};
#[test_case]
fn it_works2() {
    println!("test2... ");
    assert_eq!(1, 2);
}

#[test_case]
#[allow(clippy::eq_op)]
fn it_works1() {
    println!("test1... ");
    assert_eq!(1, 1);
}

#[test_case]
fn test_uart() {
    // map uart data register for using.
    let uart_data_reg = iomap(0x9000000.into(), 0x1000);

    let _fdt = get_device_tree().unwrap();

    // write to uart, then it will be print to the screen.
    unsafe {
        uart_data_reg.write_volatile(b'A');
        uart_data_reg.write_volatile(b'\n');
    }

    println!("uart test passed!");
}
