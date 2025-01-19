#![no_std]
#![no_main]
#![feature(naked_functions)]
#![feature(used_with_arg)]
#![feature(stmt_expr_attributes)]

extern crate alloc;

extern crate sparreal_kernel;

#[cfg_attr(target_arch = "aarch64", path = "arch/aarch64/mod.rs")]
pub mod arch;
mod config;
mod debug;
pub(crate) mod mem;
pub mod prelude;

// We export this static with an informative name so that if an application attempts to link
// two copies of cortex-m-rt together, linking will fail. We also declare a links key in
// Cargo.toml which is the more modern way to solve the same problem, but we have to keep
// __ONCE__ around to prevent linking with versions before the links key was added.
#[unsafe(export_name = "error: sparreal-rt appears more than once in the dependency graph")]
#[doc(hidden)]
pub static __ONCE__: () = ();
