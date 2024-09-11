mod boot;
mod driver;
mod mmu;
mod trap;

use core::{arch::asm, ptr::NonNull};

use aarch64_cpu::registers::*;
use page_table_interface::PhysAddr;
use sparreal_kernel::{platform::Mmu, Platform};

pub struct PlatformImpl;

impl Platform for PlatformImpl {
    fn wait_for_interrupt() {
        aarch64_cpu::asm::wfi();
    }

    fn current_ticks() -> u64 {
        CNTPCT_EL0.get()
    }

    fn tick_hz() -> u64 {
        CNTFRQ_EL0.get()
    }
}

impl Mmu for PlatformImpl {
    fn set_kernel_page_table(table: Self::Table) {
        TTBR1_EL1.set_baddr(table.paddr().as_usize() as _);
        Self::flush_tlb(None);
    }

    fn set_user_page_table(table: Option<Self::Table>) {
        match table {
            Some(tb) => TTBR0_EL1.set_baddr(tb.paddr().as_usize() as _),
            None => TTBR0_EL1.set_baddr(0),
        }
        Self::flush_tlb(None);
    }

    fn flush_tlb(addr: Option<NonNull<u8>>) {
        unsafe {
            if let Some(vaddr) = addr {
                asm!("tlbi vaae1is, {}; dsb sy; isb", in(reg) vaddr.as_ptr() as usize)
            } else {
                // flush the entire TLB
                asm!("tlbi vmalle1; dsb sy; isb")
            };
        }
    }

    fn get_kernel_page_table() -> Self::Table {
        let paddr = TTBR1_EL1.get_baddr();
        mmu::PageTable::from_addr(PhysAddr::from(paddr as usize), 4)
    }

    type Table = mmu::PageTable;
}
