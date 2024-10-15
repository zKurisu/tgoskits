#![no_std]

mod meta;
mod define;
pub mod error;
mod fdt;
mod node;
mod read;

use define::*;

pub use fdt::Fdt;
