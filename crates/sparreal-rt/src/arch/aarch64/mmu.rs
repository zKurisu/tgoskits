use core::arch::asm;

use aarch64_cpu::registers::*;
use page_table_arm::{MAIRDefault, MAIRKind, MAIRSetting, PTEFlags, PTE};
use page_table_generic::*;
use sparreal_kernel::platform::PlatformPageTable;
use sparreal_macros::api_impl;

use super::boot::BOOT_INFO;

extern "C" {
    fn _skernel();
    fn _stack_top();
    fn _ekernel();
}

pub unsafe fn init_boot_table() -> u64 {
    let table = match sparreal_kernel::mem::mmu::new_boot_table(BOOT_INFO.to_boot_config()) {
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

        let privilege = &config.setting.privilege_access;

        if !config.setting.is_global {
            flags |= PTEFlags::NG;
        }

        if privilege.readable() {
            flags |= PTEFlags::AF;
        }

        if !privilege.writable() {
            flags |= PTEFlags::AP_RO;
        }

        if !privilege.executable() {
            flags |= PTEFlags::PXN;
        }

        let user = &config.setting.user_access;

        if user.readable() {
            flags |= PTEFlags::AP_EL0;
        }

        if user.writable() {
            flags |= PTEFlags::AP_EL0;
            flags.remove(PTEFlags::AP_RO);
        }

        if !user.executable() {
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
        let mut privilege_access = AccessSetting::empty();
        let mut user_access = AccessSetting::empty();
        let mut cache_setting = CacheSetting::Normal;
        let is_global = !flags.contains(PTEFlags::NG);

        if is_valid {
            let mair_idx = pte.get_mair_idx();

            cache_setting = match MAIRDefault::from_idx(mair_idx) {
                MAIRKind::Device => CacheSetting::Device,
                MAIRKind::Normal => CacheSetting::Normal,
                MAIRKind::NonCache => CacheSetting::NonCache,
            };

            if flags.contains(PTEFlags::AF) {
                privilege_access |= AccessSetting::Read;
            }

            if !flags.contains(PTEFlags::AP_RO) {
                privilege_access |= AccessSetting::Write;
            }

            if !flags.contains(PTEFlags::PXN) {
                privilege_access |= AccessSetting::Execute;
            }

            if flags.contains(PTEFlags::AP_EL0) {
                user_access |= AccessSetting::Read;

                if !flags.contains(PTEFlags::AP_RO) {
                    user_access |= AccessSetting::Write;
                }
            }

            if !flags.contains(PTEFlags::UXN) {
                user_access |= AccessSetting::Execute;
            }
        }

        PTEGeneric {
            paddr,
            is_block,
            is_valid,
            setting: PTESetting {
                is_global,
                privilege_access,
                user_access,
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
