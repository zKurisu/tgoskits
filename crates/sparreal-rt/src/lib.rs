#![no_std]

extern crate alloc;

#[cfg_attr(target_arch = "aarch64", path = "arch/aarch64/mod.rs")]
pub mod arch;
mod consts;
mod drivers;
mod memory;

pub use sparreal_kernel::*;

unsafe fn boot(kconfig: kernel::KernelConfig) -> ! {
    kernel::init_log_and_memory(&kconfig);
    kernel::driver_register_append(drivers::registers());
    kernel::run()
}

pub fn shutdown() -> ! {
    unsafe {
        arch::PlatformImpl::shutdown();
    }
    unreachable!()
}
