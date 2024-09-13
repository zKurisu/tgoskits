use core::ptr::NonNull;

use aarch64_cpu::registers::*;
use log::debug;
use sparreal_kernel::mem::mmu;

extern "C" {
    fn _skernel();
    fn _stack_top();
}

pub type PageTable = page_table_interface::PageTableRef<'static, page_table::PTE, 512, 4>;

pub unsafe fn init_boot_table(va_offset: usize, dtb_addr: NonNull<u8>) -> u64 {
    let heap_lma = NonNull::new_unchecked(_stack_top as *mut u8);
    let kernel_lma = NonNull::new_unchecked(_skernel as *mut u8);

    let table = mmu::boot_init::<PageTable>(va_offset, dtb_addr, heap_lma, kernel_lma).unwrap();

    MAIR_EL1.set(page_table::AttrIndex::mair_value());

    table.paddr() as _
}
