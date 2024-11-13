use core::arch::asm;

use aarch64_cpu::registers::*;
use mmu::{BootTableConfig, MemoryReservedRange};
use page_table_arm::{MAIRDefault, MAIRKind, MAIRSetting, PTEFlags, PTE};
use page_table_generic::*;
use sparreal_kernel::{kernel::KernelConfig, mem::*, platform::PlatformPageTable};
use sparreal_macros::api_impl;

extern "C" {
    fn _skernel();
    fn _stack_top();
    fn _ekernel();
}

pub unsafe fn init_boot_table(kconfig: &KernelConfig) -> u64 {
    let mut reserved_memory = [None; 24];

    if let Some(reg) = kconfig.early_debug_reg {
        reserved_memory[0] = Some(MemoryReservedRange {
            start: reg.start,
            size: reg.size,
            access: AccessSetting::PrivilegeRead | AccessSetting::PrivilegeWrite,
            cache: CacheSetting::Device,
        });
    }

    let table = match sparreal_kernel::mem::mmu::new_boot_table(BootTableConfig {
        main_memory: kconfig.main_memory,
        main_memory_heap_offset: kconfig.main_memory_heap_offset,
        hart_stack_size: kconfig.hart_stack_size,
        reserved_memory,
    }) {
        Ok(t) => t,
        Err(e) => panic!("MMU init failed {:?}", e),
    };

    MAIRDefault::mair_el1_apply();

    table as _
}

pub struct PageTableImpl;

#[api_impl]
impl PlatformPageTable for PageTableImpl {
    fn flush_tlb(addr: Option<*const u8>) {
        unsafe {
            if let Some(vaddr) = addr {
                asm!("tlbi vaae1is, {}; dsb nsh; isb", in(reg) vaddr as usize)
            } else {
                // flush the entire TLB
                asm!("tlbi vmalle1; dsb nsh; isb")
            };
        }
    }

    fn page_size() -> usize {
        0x1000
    }

    fn table_level() -> usize {
        4
    }

    fn new_pte(config: PTEGeneric) -> usize {
        let mut pte = PTE::from_paddr(config.paddr);
        let mut flags = PTEFlags::empty();

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
        TTBR1_EL1.set_baddr(addr as _);
        Self::flush_tlb(None);
    }

    fn get_kernel_table() -> usize {
        TTBR1_EL1.get_baddr() as _
    }

    fn set_user_table(addr: usize) {
        TTBR0_EL1.set_baddr(addr as _);
        Self::flush_tlb(None);
    }

    fn get_user_table() -> usize {
        TTBR0_EL1.get_baddr() as _
    }
}
