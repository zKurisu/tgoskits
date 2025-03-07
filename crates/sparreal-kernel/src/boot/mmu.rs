use core::sync::atomic::{Ordering, fence};

use super::__start;
use crate::{
    globals::{self, global_val},
    io::print::*,
    mem::{mmu::*, region::init_boot_rsv_region, stack_top},
    platform::{PlatformInfoKind, regsions},
    platform_if::MMUImpl,
};

pub fn start(text_va_offset: usize, platform_info: PlatformInfoKind) -> Result<(), &'static str> {
    early_dbgln("Booting up");
    unsafe {
        set_text_va_offset(text_va_offset);
        init_boot_rsv_region();
    }

    if let Err(e) = unsafe { globals::setup(platform_info) } {
        early_dbgln("setup globle error: ");
        early_dbgln(e);
    }
    let table = new_boot_table()?;

    fence(Ordering::SeqCst);

    set_user_table(table);
    set_kernel_table(table);

    let stack_top = stack_top();

    let jump_to = __start as usize + text_va_offset;

    early_dbgln("begin enable mmu");

    early_dbg("Jump to __start: ");
    early_dbg_hex(jump_to as _);
    early_dbg(", stack top: ");
    early_dbg_hexln(stack_top as _);

    flush_tlb_all();

    fence(Ordering::SeqCst);
    MMUImpl::enable_mmu(stack_top, jump_to)
}
