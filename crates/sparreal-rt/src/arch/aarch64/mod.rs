use aarch64_cpu::registers::*;
use sparreal_kernel::{
    globals::global_val, mem::KernelRegions, platform::PlatformInfoKind, platform_if::*, println,
};
use sparreal_macros::api_impl;

use crate::mem::driver_registers;

mod boot;
mod gic;
pub(crate) mod mmu;
mod psci;
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

    fn wait_for_interrupt() {
        aarch64_cpu::asm::wfi();
    }

    fn shutdown() -> ! {
        psci::system_off()
    }

    fn debug_put(b: u8) {
        crate::debug::put(b);
    }

    fn current_ticks() -> u64 {
        CNTPCT_EL0.get()
    }

    fn tick_hz() -> u64 {
        CNTFRQ_EL0.get()
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

    fn driver_registers() -> DriverRegisterListRef {
        DriverRegisterListRef::from_raw(driver_registers())
    }
}
