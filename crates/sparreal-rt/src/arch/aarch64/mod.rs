mod boot;
mod driver;
mod mmu;
mod trap;

use core::arch::asm;

use aarch64_cpu::{asm::barrier, registers::*};
use sparreal_kernel::Platform;

pub struct PlatformImpl;

impl Platform for PlatformImpl {
    fn wait_for_interrupt() {
        aarch64_cpu::asm::wfi();
    }

    type Page = mmu::PageTable;

    fn set_kernel_page_table(table: Self::Page) {
        TTBR1_EL1.set_baddr(table.paddr().as_usize() as _);
        unsafe {
            asm!("tlbi vmalle1; dsb sy; isb");
        }
    }
}
