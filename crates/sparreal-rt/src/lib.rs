#![no_std]

extern crate alloc;

#[cfg_attr(target_arch = "aarch64", path = "arch/aarch64/mod.rs")]
pub mod arch;
mod consts;
mod device_tree;
mod driver;
mod kernel;
mod lang_items;
pub mod mem;

use log::debug;
pub use sparreal_kernel::*;


unsafe fn boot(kconfig: KernelConfig)->!{
    init_log_and_memory(&kconfig);       
    
    unreachable!()
}