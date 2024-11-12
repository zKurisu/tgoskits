use core::ptr::NonNull;

use super::*;
use crate::platform::{self, table_level};
use log::debug;
use page_table_generic::PageTableRef;
pub use page_table_generic::*;
mod table;

use table::{get_kernal_table, PageTableRef};

struct BootInfo {
    va_offset: usize,
}

#[link_section = ".data.boot"]
static mut BOOT_INFO: BootInfo = BootInfo { va_offset: 0 };

pub(super) unsafe fn set_va_offset(offset: usize) {
    BOOT_INFO.va_offset = offset;
}

pub fn va_offset() -> usize {
    unsafe { BOOT_INFO.va_offset }
}

pub(crate) unsafe fn init_table(
    kconfig: &KernelConfig,
    access: &mut PageAllocatorRef,
) -> PagingResult<()> {
    debug!("Initializing page table...");

    let table = PageTableRef::create_empty(access)?;

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
            MapConfig::new(
                virt.as_mut_ptr(),
                memory.start.as_usize(),
                AccessSetting::PrivilegeRead
                    | AccessSetting::PrivilegeWrite
                    | AccessSetting::PrivilegeExecute,
                CacheSetting::Normal,
            ),
            size,
            true,
            access,
        )?;
    }

    let virt = kconfig.main_memory.start.to_virt();
    debug!(
        "Map memory {:#X} -> {:#X} size {:#X}",
        virt.as_usize(),
        kconfig.main_memory.start.as_usize(),
        kconfig.main_memory.size
    );

    table.map_region(
        MapConfig::new(
            virt.as_mut_ptr(),
            kconfig.main_memory.start.as_usize(),
            AccessSetting::PrivilegeRead
                | AccessSetting::PrivilegeWrite
                | AccessSetting::PrivilegeExecute,
            CacheSetting::Normal,
        ),
        kconfig.main_memory.size,
        true,
        access,
    )?;

    if let Some(debug_reg) = &kconfig.early_debug_reg {
        let virt = debug_reg.start.to_virt();
        debug!(
            "Map debug register {:#X} -> {:#X} size {:#X}",
            virt.as_usize(),
            debug_reg.start.as_usize(),
            debug_reg.size
        );
        table.map_region(
            MapConfig::new(
                virt.as_mut_ptr(),
                debug_reg.start.as_usize(),
                AccessSetting::PrivilegeRead | AccessSetting::PrivilegeWrite,
                CacheSetting::Device,
            ),
            debug_reg.size,
            true,
            access,
        )?;
    }

    platform::set_kernel_page_table(table.paddr().into());
    platform::set_user_page_table(None);

    debug!("Done!");
    Ok(())
}

pub fn iomap(paddr: PhysAddr, size: usize) -> NonNull<u8> {
    unsafe {
        let table = get_kernal_table();
        let paddr = paddr.align_down(0x1000);
        let vaddr = paddr.to_virt().as_mut_ptr();
        let size = size.max(0x1000);

        let heap = HEAP_ALLOCATOR.write();
        let mut heap_mut = PageAllocatorRef::new(heap);

        table.map_region_with_handle(
            MapConfig::new(
                virt.as_mut_ptr(),
                debug_reg.start.as_usize(),
                AccessSetting::PrivilegeRead | AccessSetting::PrivilegeWrite,
                CacheSetting::Device,
            ),
            debug_reg.size,
            true,
            &mut heap_mut,
            Some(&|p| {
                platform::flush_tlb(Some(p.into()));
            }),
        )?;

        NonNull::new_unchecked(vaddr)
    }
}
