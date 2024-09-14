use core::ptr::NonNull;

use aarch64_cpu::registers::*;
use log::debug;
use page_table_interface::{MapConfig, PageTableFn};
use sparreal_kernel::{
    mem::{mmu, Addr, Virt},
    util,
};

use crate::consts::BYTES_1G;

use super::{
    debug::{debug_hex, debug_print},
    PlatformImpl,
};

extern "C" {
    fn _skernel();
    fn _stack_top();
}

pub type PageTable = page_table_interface::PageTableRef<'static, page_table::PTE, 512, 4>;

pub unsafe fn init_boot_table(va_offset: usize, dtb_addr: NonNull<u8>) -> u64 {
    let heap_lma = NonNull::new_unchecked(_stack_top as *mut u8);
    let kernel_lma = NonNull::new_unchecked(_skernel as *mut u8);
    let kernel_size = heap_lma.as_ptr() as usize - kernel_lma.as_ptr() as usize;

    debug_print("kernel @");
    debug_hex(kernel_lma.as_ptr() as usize as _);
    debug_print(" heap @");
    debug_hex(heap_lma.as_ptr() as usize as _);
    debug_print("\r\n");

    let table =
        mmu::boot_init::<PlatformImpl>(va_offset, dtb_addr, kernel_lma, kernel_size).unwrap();

    MAIR_EL1.set(page_table::AttrIndex::mair_value());

    table.paddr() as _
}
