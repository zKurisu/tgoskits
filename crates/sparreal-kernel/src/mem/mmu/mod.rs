use core::{
    alloc::Layout,
    ptr::{slice_from_raw_parts, slice_from_raw_parts_mut, NonNull},
};

use super::*;
use flat_device_tree::Fdt;
use log::debug;
pub use page_table_interface::*;

use crate::{
    driver::device_tree::{self, get_device_tree, set_dtb_addr},
    kernel,
    platform::Mmu,
    util::{
        self,
        boot::{k_boot_debug, k_boot_debug_hex},
    },
    Platform,
};

struct BootInfo {
    va_offset: usize,
    reserved_start: usize,
    reserved_end: usize,
}

#[link_section = ".data.boot"]
static mut BOOT_INFO: BootInfo = BootInfo {
    va_offset: 0,
    reserved_start: 0,
    reserved_end: 0,
};

fn reserved_start() -> Phys<u8> {
    unsafe { BOOT_INFO.reserved_start.into() }
}
fn reserved_end() -> Phys<u8> {
    unsafe { BOOT_INFO.reserved_end.into() }
}

pub fn va_offset() -> usize {
    unsafe { BOOT_INFO.va_offset }
}



pub(crate) unsafe fn init_page_table<P: Platform>(
    kconfig: &KernelConfig,
    access: &mut impl Access,
) -> Result<(), PagingError> {
    debug!("Initializing page table...");
    let mut table = P::Table::new(access)?;

    if let Some(memory) = &kconfig.reserved_memory {
        let virt = memory.start.to_virt();
        let size = memory.size.align_up(BYTES_1M * 2);
        debug!(
            "Map reserved memory region {:#X} -> {:#X}  size: {:#X}",
            virt.as_usize(),
            memory.start.as_usize(),
            size,
        );
        table.map_region(
            MapConfig {
                vaddr: virt.as_mut_ptr(),
                paddr: memory.start.as_usize(),
                attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
            },
            size,
            true,
            access,
        );
    }

    let virt = kconfig.main_memory.start.to_virt();
    debug!(
        "Map memory {:#X} -> {:#X} size {:#X}",
        virt.as_usize(),
        kconfig.main_memory.start.as_usize(),
        kconfig.main_memory.size
    );

    table.map_region(
        MapConfig {
            vaddr: virt.as_mut_ptr(),
            paddr: kconfig.main_memory.start.as_usize(),
            attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
        },
        kconfig.main_memory.size,
        true,
        access,
    );

    if let Some(debug_reg) = &kconfig.early_debug_reg {
        let virt = debug_reg.start.to_virt();
        debug!(
            "Map debug register {:#X} -> {:#X} size {:#X}",
            virt.as_usize(),
            debug_reg.start.as_usize(),
            debug_reg.size
        );
        table.map_region(
            MapConfig {
                vaddr: virt.as_mut_ptr(),
                paddr: debug_reg.start.as_usize(),
                attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Device,
            },
            debug_reg.size,
            true,
            access,
        );
    }

    P::set_kernel_page_table(&table);
    P::set_user_page_table(None);
    Ok(())
}

pub(crate) unsafe fn iomap<P: Platform>(paddr: PhysAddr, size: usize) -> NonNull<u8> {
    let mut table = P::get_kernel_page_table();
    let paddr = paddr.align_down(0x1000);
    let vaddr = paddr.to_virt().as_mut_ptr();
    let size = size.max(0x1000);

    let mut heap = HEAP_ALLOCATOR.lock();
    let mut heap_mut = AllocatorRef::new(&mut heap);

    let _ = table.map_region_with_handle(
        MapConfig {
            vaddr,
            paddr: paddr.into(),
            attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Device,
        },
        size,
        true,
        &mut heap_mut,
        Some(&|addr| {
            P::flush_tlb(Some(addr));
        }),
    );

    NonNull::new_unchecked(vaddr)
}
