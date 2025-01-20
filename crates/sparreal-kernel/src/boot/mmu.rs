use core::sync::atomic::{Ordering, fence};

use super::{__start, print_info};
use crate::{
    globals::global_val,
    io::print::*,
    mem::{mmu::*, set_va_offset_now},
    platform::PlatformInfoKind,
    platform_if::MMUImpl,
};

pub fn start(
    va_offset: usize,
    platform_info: PlatformInfoKind,
    rsv_memory: &[BootMemoryRegion],
) -> Result<(), &'static str> {
    early_dbgln("Booting up");

    crate::mem::set_va_offset(va_offset);
    unsafe { crate::globals::setup(platform_info)? };

    print_info();

    let table = new_boot_table(rsv_memory)?;

    fence(Ordering::SeqCst);

    set_user_table(table);
    set_kernel_table(table);

    flush_tlb_all();

    fence(Ordering::SeqCst);

    let jump_to = __start as usize + va_offset;
    let stack_top = global_val().kstack_top.as_usize() + va_offset;
    unsafe { set_va_offset_now(va_offset) };

    early_dbg("Jump to __start: ");
    early_dbg_hex(jump_to as _);
    early_dbg(", stack top: ");
    early_dbg_hexln(stack_top as _);

    MMUImpl::enable_mmu(stack_top, jump_to)
}
