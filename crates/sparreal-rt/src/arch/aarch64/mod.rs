mod boot;
mod debug;
mod mmu;
mod trap;

use core::{arch::asm, ptr::NonNull};

use aarch64_cpu::registers::*;
use alloc::boxed::Box;
use debug::DebugWriter;
use mmu::PageTable;
use page_table_interface::{Access, MapConfig, PageTableFn, PagingResult};
use sparreal_kernel::{
    mem::{PageAllocatorRef, Phys, Virt},
    platform::{Mmu, Platform2},
    Platform,
};
use sparreal_macros::api_impl;

static mut VA_OFFSET: usize = 0;

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
    fn set_kernel_page_table(table: &Self::Table) {
        TTBR1_EL1.set_baddr(table.paddr() as _);
        Self::flush_tlb(None);
    }

    fn set_user_page_table(table: Option<&Self::Table>) {
        match table {
            Some(tb) => TTBR0_EL1.set_baddr(tb.paddr() as _),
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
        mmu::PageTable::from_addr(paddr as usize, 4)
    }

    type Table = mmu::PageTable;

    fn boot_debug_writer() -> Option<impl core::fmt::Write> {
        Some(DebugWriter {})
    }
}

pub struct Platform2Impl;

#[api_impl]
impl Platform2 for Platform2Impl {
    unsafe fn wait_for_interrupt() {
        aarch64_cpu::asm::wfi();
    }

    unsafe fn current_ticks() -> u64 {
        CNTPCT_EL0.get()
    }

    unsafe fn tick_hz() -> u64 {
        CNTFRQ_EL0.get()
    }

    unsafe fn debug_write_char(ch: char) {
        unsafe { debug::put_debug(ch as u8) };
    }

    unsafe fn table_new(access: &mut PageAllocatorRef) -> PagingResult<Phys<u8>> {
        PageTable::new(access).map(|table| table.paddr().into())
    }

    unsafe fn table_map(
        table: Phys<u8>,
        config: MapConfig,
        size: usize,
        allow_block: bool,
        flush: bool,
        access: &mut PageAllocatorRef,
    ) -> PagingResult<()> {
        let mut table = PageTable::from_addr(table.into(), 4);
        if flush {
            table.map_region_with_handle(
                config,
                size,
                allow_block,
                access,
                Some(&|addr: *const u8| {
                    Platform2Impl::flush_tlb(Some(addr.into()));
                }),
            )
        } else {
            table.map_region(config, size, allow_block, access)
        }
    }

    unsafe fn set_kernel_page_table(table: Phys<u8>) {
        TTBR1_EL1.set_baddr(table.as_usize() as _);
        Self::flush_tlb(None);
    }

    unsafe fn set_user_page_table(table: Option<Phys<u8>>) {
        match table {
            Some(tb) => TTBR0_EL1.set_baddr(tb.as_usize() as _),
            None => TTBR0_EL1.set_baddr(0),
        }
        Self::flush_tlb(None);
    }

    unsafe fn get_kernel_page_table() -> Phys<u8> {
        let paddr = TTBR1_EL1.get_baddr();
        (paddr as usize).into()
    }

    unsafe fn flush_tlb(addr: Option<Virt<u8>>) {
        if let Some(vaddr) = addr {
            asm!("tlbi vaae1is, {}; dsb sy; isb", in(reg) vaddr.as_usize())
        } else {
            // flush the entire TLB
            asm!("tlbi vmalle1; dsb sy; isb")
        };
    }
}
