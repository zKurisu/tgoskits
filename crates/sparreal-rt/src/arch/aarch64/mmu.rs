use core::{alloc::Layout, arch::asm, cell::UnsafeCell, ptr::NonNull, sync::atomic::AtomicU64};

use aarch64::{DescriptorAttr, PTE};
use aarch64_cpu::{asm::barrier, registers::*};
use page_table::*;
use page_table_interface::{MapConfig, PageAttribute, PageTableMap};
use sparreal_kernel::mem::mmu;
use tock_registers::interfaces::ReadWriteable;

use crate::KernelConfig;

const BYTES_1G: usize = 1024 * 1024 * 1024;


pub type PageTableRef = page_table::PageTableRef<PTE, 4>;

extern "C" {
    fn _skernel();
    fn _stack_top();
}


type BootTable = page_table_interface::PageTableRef<'static, page_table2::PTE, 512, 4>;

pub unsafe fn init_boot_table(va_offset: usize, dtb_addr: NonNull<u8>) -> u64 {
    let heap_lma = NonNull::new_unchecked(_stack_top as *mut u8);
    let kernel_lma = NonNull::new_unchecked(_skernel as *mut u8);

    let table = mmu::boot_init::<BootTable>(va_offset, dtb_addr, heap_lma, kernel_lma).unwrap();

    MAIR_EL1.set(page_table2::AttrIndex::mair_value());

    table.paddr().as_usize() as _
}