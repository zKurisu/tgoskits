#![no_std]

extern crate alloc;

#[cfg_attr(target_arch = "aarch64", path = "arch/aarch64/mod.rs")]
pub mod arch;
mod consts;
mod drivers;
pub mod mem;

pub use sparreal_kernel::*;

unsafe fn boot(kconfig: kernel::KernelConfig) -> ! {
    kernel::init_log_and_memory(&kconfig);
    kernel::driver_manager().register_all(drivers::registers());
    kernel::run()
}
