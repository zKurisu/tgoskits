use core::ptr::NonNull;

use aarch64_cpu::registers::*;
use flat_device_tree::Fdt;
use page_table_interface::{Access, MapConfig, PageAttribute, PageTableFn};
use sparreal_kernel::{mem::*, util, KernelConfig};

use crate::consts::*;

use super::{
    debug::{debug_hex, debug_print},
    PlatformImpl, VA_OFFSET,
};

extern "C" {
    fn _skernel();
    fn _stack_top();
    fn _ekernel();
}

pub type PageTable = page_table_interface::PageTableRef<'static, page_table::PTE, 512, 4>;

pub unsafe fn init_boot_table(va_offset: usize, kconfig: &KernelConfig) -> u64 {
    let heap_size =
        (kconfig.main_memory.size - kconfig.main_memory_heap_offset - kconfig.hart_stack_size) / 2;
    let heap_start = kconfig.main_memory.start + kconfig.main_memory_heap_offset + heap_size;

    debug_print("Page Allocator [");
    debug_hex(heap_start.as_usize() as _);
    debug_print(", ");
    debug_hex((heap_start.as_usize() + heap_size) as _);
    debug_print(")\r\n");

    let mut access = PageAllocator::new(
        NonNull::new_unchecked(heap_start.as_usize() as _),
        heap_size,
    );

    let mut table = <PageTable as PageTableFn>::new(&mut access).unwrap();

    debug_print("Table @");
    debug_hex(table.paddr() as _);
    debug_print("\r\n");

    if let Some(memory) = &kconfig.reserved_memory {
        let size = memory.size.align_up(BYTES_1G);

        map_boot_region(
            "Map reserved memory",
            &mut table,
            memory.start,
            size,
            PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
            &mut access,
        );
    }

    map_boot_region(
        "Map main memory",
        &mut table,
        kconfig.main_memory.start,
        kconfig.main_memory.size,
        PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
        &mut access,
    );

    if let Some(debug_reg) = &kconfig.early_debug_reg {
        map_boot_region(
            "Map debug reg",
            &mut table,
            debug_reg.start,
            debug_reg.size,
            PageAttribute::Read | PageAttribute::Write | PageAttribute::Device,
            &mut access,
        );
    }

    MAIR_EL1.set(page_table::AttrIndex::mair_value());

    table.paddr() as _
}

unsafe fn map_boot_region(
    name: &str,
    table: &mut PageTable,
    paddr: Phys<u8>,
    size: usize,
    attrs: PageAttribute,
    access: &mut impl Access,
) {
    let virt = paddr.as_usize() + VA_OFFSET;

    debug_print("map ");
    debug_print(name);
    debug_print(" @");
    debug_hex(virt as _);
    debug_print(" -> ");
    debug_hex(paddr.as_usize() as _);
    debug_print(" size: ");
    debug_hex(size as _);
    debug_print("\r\n");

    let _ = table.map_region(
        MapConfig {
            vaddr: virt as _,
            paddr: paddr.into(),
            attrs,
        },
        size,
        true,
        access,
    );

    let _ = table.map_region(
        MapConfig {
            vaddr: paddr.as_usize() as _,
            paddr: paddr.into(),
            attrs,
        },
        size,
        true,
        access,
    );
}
