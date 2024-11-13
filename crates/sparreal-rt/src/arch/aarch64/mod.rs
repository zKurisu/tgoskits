mod boot;
mod debug;
mod mmu;
mod psci;
mod trap;

use core::arch::asm;

use aarch64_cpu::registers::*;
use alloc::{format, string::String};
use sparreal_kernel::{driver::device_tree::get_device_tree, platform::Platform, print, println};
use sparreal_macros::api_impl;

static mut VA_OFFSET: usize = 0;

pub struct PlatformImpl;

#[api_impl]
impl Platform for PlatformImpl {
    unsafe fn shutdown() {
        psci::system_off();
    }

    unsafe fn wait_for_interrupt() {
        aarch64_cpu::asm::wfi();
    }

    unsafe fn current_ticks() -> u64 {
        CNTPCT_EL0.get()
    }

    unsafe fn tick_hz() -> u64 {
        CNTFRQ_EL0.get()
    }

    unsafe fn debug_write_char(ch: u8) {
        unsafe {
            #[cfg(feature = "early-print")]
            debug::put_debug(ch)
        };
    }

    fn print_system_info() {
        println!(
            "CPU: {}.{}.{}.{}",
            MPIDR_EL1.read(MPIDR_EL1::Aff0),
            MPIDR_EL1.read(MPIDR_EL1::Aff1),
            MPIDR_EL1.read(MPIDR_EL1::Aff2),
            MPIDR_EL1.read(MPIDR_EL1::Aff3)
        );
        let _ = print_board_info();
    }

    fn irqs_enable() {
        unsafe { asm!("msr daifclr, #2") };
    }

    fn irqs_disable() {
        unsafe { asm!("msr daifset, #2") };
    }

    fn cpu_id() -> u64 {
        MPIDR_EL1.get()
    }
    fn cpu_id_display() -> String {
        format!(
            "{}.{}.{}.{}",
            MPIDR_EL1.read(MPIDR_EL1::Aff0),
            MPIDR_EL1.read(MPIDR_EL1::Aff1),
            MPIDR_EL1.read(MPIDR_EL1::Aff2),
            MPIDR_EL1.read(MPIDR_EL1::Aff3)
        )
    }
}

fn print_board_info() -> Option<()> {
    let fdt = get_device_tree()?;
    let root = fdt.all_nodes().next()?;
    let caps = root.compatibles();

    print!("Board:");
    for cap in caps {
        print!(" {}", cap);
    }
    println!();
    Some(())
}
