use core::ptr::NonNull;

use aarch64_cpu::registers::*;
use page_table_arm::{MAIRDefault, MAIRKind, MAIRSetting, PTEFlags, PTE};
use page_table_generic::*;
use sparreal_kernel::{kernel::KernelConfig, mem::*, platform::PlatformPageTable};
use sparreal_macros::api_impl;

use crate::{debug_hex, early_debug::debug_print};

use super::VA_OFFSET;

extern "C" {
    fn _skernel();
    fn _stack_top();
    fn _ekernel();
}

pub type PageTable = page_table_generic::PageTableRef<'static, page_table_arm::PTE, 512, 4>;

pub unsafe fn init_boot_table(kconfig: &KernelConfig) -> u64 {
    let heap_size =
        (kconfig.main_memory.size - kconfig.main_memory_heap_offset - kconfig.hart_stack_size) / 2;
    let heap_start = kconfig.main_memory.start + kconfig.main_memory_heap_offset + heap_size;

    debug_print("Page Allocator [");
    debug_hex!(heap_start.as_usize());
    debug_print(", ");
    debug_hex!((heap_start.as_usize() + heap_size));
    debug_print(")\r\n");

    let mut access = PageAllocator::new(
        NonNull::new_unchecked(heap_start.as_usize() as _),
        heap_size,
    );

    let mut table = <PageTable as PageTableFn>::new(&mut access).unwrap();

    debug_print("Table @");
    debug_hex!(table.paddr());
    debug_print("\r\n");

    if let Some(memory) = &kconfig.reserved_memory {
        // sp 范围也需要涵盖
        let size = memory.size.align_up(BYTES_1G);

        map_boot_region(
            "Map reserved memory",
            &mut table,
            memory.start,
            size,
            PageAttribute::Read | PageAttribute::Write | PageAttribute::PrivilegeExecute,
            &mut access,
        );
    }

    map_boot_region(
        "Map main memory",
        &mut table,
        kconfig.main_memory.start,
        kconfig.main_memory.size,
        PageAttribute::Read | PageAttribute::Write | PageAttribute::PrivilegeExecute,
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

    MAIR_EL1.set(page_table_arm::AttrIndex::mair_value());

    table.paddr() as _
}

// unsafe fn map_boot_region(
//     name: &str,
//     table: &mut PageTable,
//     paddr: Phys<u8>,
//     size: usize,
//     attrs: PageAttribute,
//     access: &mut impl Access,
// ) {
//     let virt = paddr.as_usize() + VA_OFFSET;

//     debug_print("map ");
//     debug_print(name);
//     debug_print(" @");
//     debug_hex!(virt);
//     debug_print(" -> ");
//     debug_hex!(paddr.as_usize());
//     debug_print(" size: ");
//     debug_hex!(size);
//     debug_print("\r\n");

//     let _ = table.map_region(
//         MapConfig {
//             vaddr: virt as _,
//             paddr: paddr.into(),
//             attrs,
//         },
//         size,
//         true,
//         access,
//     );

//     let _ = table.map_region(
//         MapConfig {
//             vaddr: paddr.as_usize() as _,
//             paddr: paddr.into(),
//             attrs,
//         },
//         size,
//         true,
//         access,
//     );
// }

pub struct PageTableImpl;

#[api_impl]
impl PlatformPageTable for PageTableImpl {
    fn flush_tlb(addr: Option<*const u8>) {
        todo!()
    }

    fn page_size() -> usize {
        0x1000
    }

    fn table_level() -> usize {
        4
    }

    fn new_pte(config: PTEGeneric) -> usize {
        let mut pte = PTE::from_paddr(config.paddr);

        if config.is_valid {
            flags |= PTEFlags::VALID;
        }

        if !config.is_block {
            flags |= PTEFlags::NON_BLOCK;
        }

        pte.set_mair_idx(MAIRDefault::get_idx(match config.setting.cache_setting {
            CacheSetting::Normal => MAIRKind::Normal,
            CacheSetting::Device => MAIRKind::Device,
            CacheSetting::NonCache => MAIRKind::NonCache,
        }));

        let mut flags = PTEFlags::empty();

        let access = &config.setting.access_setting;

        if access.contains(AccessSetting::PrivilegeRead) {
            flags |= PTEFlags::AF;
        }

        if !access.contains(AccessSetting::PrivilegeWrite) {
            flags |= PTEFlags::AP_RO;
        }

        if !access.contains(AccessSetting::PrivilegeExecute) {
            flags |= PTEFlags::PXN;
        }

        if access.contains(AccessSetting::UserRead) {
            flags |= PTEFlags::AP_EL0;
        }

        if access.contains(AccessSetting::UserWrite) {
            flags |= PTEFlags::AP_EL0;
            flags.remove(PTEFlags::AP_RO);
        }

        if !access.contains(AccessSetting::UserExecute) {
            flags |= PTEFlags::UXN;
        }

        pte.set_flags(flags);

        let out: u64 = pte.into();

        out as _
    }

    fn read_pte(pte: usize) -> PTEGeneric {
        let pte = PTE::from(pte as u64);
        let paddr = pte.paddr();
        let flags = pte.get_flags();
        let is_valid = flags.contains(PTEFlags::VALID);
        let is_block = !flags.contains(PTEFlags::NON_BLOCK);
        let mut access_setting = AccessSetting::empty();
        let mut cache_setting = CacheSetting::Normal;

        if is_valid {
            let mair_idx = pte.get_mair_idx();

            cache_setting = match MAIRDefault::from_idx(mair_idx) {
                MAIRKind::Device => CacheSetting::Device,
                MAIRKind::Normal => CacheSetting::Normal,
                MAIRKind::NonCache => CacheSetting::NonCache,
            };

            if flags.contains(PTEFlags::AF) {
                access_setting |= AccessSetting::PrivilegeRead;
            }

            if !flags.contains(PTEFlags::AP_RO) {
                access_setting |= AccessSetting::PrivilegeWrite;
            }

            if !flags.contains(PTEFlags::PXN) {
                access_setting |= AccessSetting::PrivilegeExecute;
            }

            if flags.contains(PTEFlags::AP_EL0) {
                access_setting |= AccessSetting::UserRead;

                if !flags.contains(PTEFlags::AP_RO) {
                    access_setting |= AccessSetting::UserWrite;
                }

                if !flags.contains(PTEFlags::UXN) {
                    access_setting |= AccessSetting::UserExecute;
                }
            }
        }

        PTEGeneric {
            paddr,
            is_block,
            is_valid,
            setting: PTESetting {
                access_setting,
                cache_setting,
            },
        }
    }

    fn set_kernel_table(addr: usize) {
        todo!()
    }

    fn get_kernel_table() -> usize {
        todo!()
    }

    fn set_user_table(addr: usize) {
        todo!()
    }

    fn get_user_table() -> usize {
        todo!()
    }
}
