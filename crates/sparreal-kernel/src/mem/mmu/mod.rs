use core::ptr::NonNull;

use super::*;
use log::debug;
pub use page_table_interface::*;

use crate::platform;

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

    let table = platform::table_new(access)?;

    if let Some(memory) = &kconfig.reserved_memory {
        let virt = memory.start.to_virt();
        let size = memory.size.align_up(BYTES_1M * 2);
        debug!(
            "Map reserved memory region {:#X} -> {:#X}  size: {:#X}",
            virt.as_usize(),
            memory.start.as_usize(),
            size,
        );
        platform::table_map(
            table,
            MapConfig {
                vaddr: virt.as_mut_ptr(),
                paddr: memory.start.as_usize(),
                attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
            },
            size,
            true,
            false,
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

    platform::table_map(
        table,
        MapConfig {
            vaddr: virt.as_mut_ptr(),
            paddr: kconfig.main_memory.start.as_usize(),
            attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
        },
        kconfig.main_memory.size,
        true,
        false,
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
        platform::table_map(
            table,
            MapConfig {
                vaddr: virt.as_mut_ptr(),
                paddr: debug_reg.start.as_usize(),
                attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Device,
            },
            debug_reg.size,
            true,
            false,
            access,
        )?;
    }

    platform::set_kernel_page_table(table);
    platform::set_user_page_table(None);

    debug!("Done!");
    Ok(())
}

pub(crate) fn iomap(paddr: PhysAddr, size: usize) -> NonNull<u8> {
    unsafe {
        let table = platform::get_kernel_page_table();
        let paddr = paddr.align_down(0x1000);
        let vaddr = paddr.to_virt().as_mut_ptr();
        let size = size.max(0x1000);

        let heap = HEAP_ALLOCATOR.write();
        let mut heap_mut = PageAllocatorRef::new(heap);

        let _ = platform::table_map(
            table,
            MapConfig {
                vaddr,
                paddr: paddr.into(),
                attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Device,
            },
            size,
            true,
            true,
            &mut heap_mut,
        );

        NonNull::new_unchecked(vaddr)
    }
}
