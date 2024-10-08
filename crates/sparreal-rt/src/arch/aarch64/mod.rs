mod boot;
mod debug;
mod mmu;
mod trap;

use core::arch::asm;

use aarch64_cpu::registers::*;
use alloc::{format, string::String};
use mmu::PageTable;
use page_table_interface::{MapConfig, PageTableFn, PagingResult};
use sparreal_kernel::{
    driver::device_tree::get_device_tree, mem::*, platform::Platform, print, println,
};
use sparreal_macros::api_impl;

static mut VA_OFFSET: usize = 0;

pub struct PlatformImpl;

#[api_impl]
impl Platform for PlatformImpl {
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
        unsafe { debug::put_debug(ch) };
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
                    PlatformImpl::flush_tlb(Some(addr.into()));
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
    let root = fdt.root().ok()?;
    let caps = root.compatible().all();

    print!("Board:");
    for cap in caps {
        print!(" {}", cap);
    }
    println!();
    Some(())
}
