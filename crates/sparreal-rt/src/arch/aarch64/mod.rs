use core::{arch::asm, ffi::c_void};

use aarch64_cpu::registers::*;
use sparreal_kernel::{
    globals::global_val, mem::KernelRegions, platform::PlatformInfoKind, platform_if::*, println,
};
use sparreal_macros::api_impl;

use crate::mem::driver_registers;

mod boot;
mod cache;
mod gic;
pub(crate) mod mmu;
mod psci;
mod timer;
mod trap;

pub(crate) fn cpu_id() -> usize {
    const CPU_ID_MASK: u64 = 0xFF_FFFF + (0xFFFF_FFFF << 32);
    (aarch64_cpu::registers::MPIDR_EL1.get() & CPU_ID_MASK) as usize
}

struct PlatformImpl;

#[api_impl]
impl Platform for PlatformImpl {
    fn kstack_size() -> usize {
        crate::config::KERNEL_STACK_SIZE
    }

    fn kernel_regions() -> KernelRegions {
        crate::mem::kernel_regions()
    }

    fn cpu_id() -> usize {
        cpu_id()
    }

    fn cpu_context_size() -> usize {
        size_of::<trap::Context>()
    }

    unsafe fn cpu_context_sp(ctx_ptr: *const u8) -> usize {
        let ctx = unsafe { &*(ctx_ptr as *const trap::Context) };
        ctx.sp
    }

    unsafe fn get_current_tcb_addr() -> *const u8 {
        SP_EL0.get() as usize as _
    }

    unsafe fn set_current_tcb_addr(addr: *const u8) {
        SP_EL0.set(addr as usize as _);
    }

    unsafe fn cpu_context_init(ctx_ptr: *mut u8, pc: *const c_void, stack_top: *const u8) {
        unsafe {
            let ctx = &mut *(ctx_ptr as *mut trap::Context);
            ctx.spsr = SPSR_EL1.get();
            ctx.pc = pc as usize;
            ctx.sp = stack_top as usize;
        }
    }
    unsafe fn cpu_context_switch(prev: *mut u8, next: *mut u8) {
        unsafe {
            let prev = &mut *(prev as *mut trap::Context);
            let next = &mut *(next as *mut trap::Context);
            prev.switch_to(next);
        }
    }

    fn wait_for_interrupt() {
        aarch64_cpu::asm::wfi();
    }

    fn shutdown() -> ! {
        psci::system_off()
    }

    fn debug_put(b: u8) {
        crate::debug::put(b);
    }

    fn irq_all_enable() {
        unsafe { asm!("msr daifclr, #2") };
    }
    fn irq_all_disable() {
        unsafe { asm!("msr daifset, #2") };
    }
    fn irq_all_is_enabled() -> bool {
        let c = DAIF.read(DAIF::I);
        c > 0
    }

    fn on_boot_success() {
        match &global_val().platform_info {
            PlatformInfoKind::DeviceTree(fdt) => {
                if let Err(e) = psci::setup_method_by_fdt(fdt.get()) {
                    println!("{}", e);
                }
            }
        }
    }

    fn dcache_range(op: CacheOp, addr: usize, size: usize) {
        cache::dcache_range(op, addr, size);
    }

    fn driver_registers() -> DriverRegisterListRef {
        DriverRegisterListRef::from_raw(driver_registers())
    }
}
