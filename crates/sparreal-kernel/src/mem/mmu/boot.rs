use core::ptr::NonNull;

use page_table_generic::{err::PagingResult, Access, AccessSetting, CacheSetting, MapConfig};

use crate::MemoryRange;

use super::{table::PageTableRef, va_offset, MemoryReservedRange, PageAllocator};

pub struct BootTableConfig {
    pub main_memory: MemoryRange,
    pub main_memory_heap_offset: usize,
    pub hart_stack_size: usize,
    pub reserved_memory: [Option<MemoryReservedRange>; 24],
}

pub fn new_boot_table(config: BootTableConfig) -> PagingResult<usize> {
    let heap_size =
        (config.main_memory.size - config.main_memory_heap_offset - config.hart_stack_size) / 2;
    let heap_start = config.main_memory.start + config.main_memory_heap_offset + heap_size;

    let mut access = unsafe {
        PageAllocator::new(
            NonNull::new_unchecked(heap_start.as_usize() as _),
            heap_size,
        )
    };

    let mut table = PageTableRef::create_empty(&mut access)?;

    unsafe {
        map_boot_region(
            &mut table,
            config.main_memory.start.into(),
            config.main_memory.size,
            AccessSetting::PrivilegeRead
                | AccessSetting::PrivilegeWrite
                | AccessSetting::PrivilegeExecute,
            CacheSetting::Normal,
            &mut access,
        )?;

        for rsv in config.reserved_memory {
            if let Some(rsv) = rsv {
                map_boot_region(
                    &mut table,
                    rsv.start.into(),
                    rsv.size,
                    rsv.access,
                    rsv.cache,
                    &mut access,
                )?;
            }
        }
    }
    Ok(table.paddr())
}

unsafe fn map_boot_region(
    table: &mut PageTableRef<'_>,
    paddr: usize,
    size: usize,
    access_setting: AccessSetting,
    cache_setting: CacheSetting,
    access: &mut impl Access,
) -> PagingResult<()> {
    table.map_region(
        MapConfig::new(paddr as _, paddr, access_setting, cache_setting),
        size,
        true,
        access,
    )?;
    let vaddr = paddr + va_offset();

    table.map_region(
        MapConfig::new(vaddr as _, paddr, access_setting, cache_setting),
        size,
        true,
        access,
    )?;

    Ok(())
}
