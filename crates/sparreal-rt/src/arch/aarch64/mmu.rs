use core::{alloc::Layout, arch::asm, cell::UnsafeCell, ptr::NonNull, sync::atomic::AtomicU64};

use aarch64_cpu::{asm::barrier, registers::*};
use page_table_interface::{MapConfig, PageAttribute, PageTableFn};
use sparreal_kernel::mem::mmu;
use tock_registers::interfaces::ReadWriteable;

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

    table.paddr().as_usize() as _
}
