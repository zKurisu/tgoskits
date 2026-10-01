//! Pure Rust driver for the SG2002 JPEG Processing Unit (JPU).
//!
//! The implementation follows CVITEK's U-Boot driver (`drivers/jpeg/`) and
//! polls for baseline JPEG hardware decode completion. It supports planar
//! output with the original sampling and direct NV12 output for 4:2:0 JPEG.

mod decoder;
mod header;
mod mem;
pub mod regs;

pub use decoder::{DecodeInfo, DecodeOutput, DecodeResult, JpuDecoder, JpuDmaToPhysFn, JpuMmio};
