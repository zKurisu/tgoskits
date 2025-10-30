#![no_std]
#![no_main]

#[macro_use]
extern crate alloc;

#[macro_use]
extern crate log;

#[cfg(target_arch = "loongarch64")]
#[path = "arch/loongarch64/mod.rs"]
pub mod arch;

#[cfg(efi)]
mod efi_stub;
mod reloc;
