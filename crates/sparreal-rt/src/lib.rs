#![no_std]

#[cfg_attr(target_arch = "aarch64", path = "arch/aarch64/mod.rs")]
pub mod arch;
mod consts;
mod kernel;
mod lang_items;
pub(crate) mod mem;

pub use sparreal_kernel::*;

pub use kernel::kernel;
