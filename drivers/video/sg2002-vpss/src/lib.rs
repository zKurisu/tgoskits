//! Portable SG2002/CV181x VPSS scaler driver core.
//!
//! Memory-backed NV12 or three-plane YUV422 enters IMG_V, is converted and
//! scaled by SC_V1, and is written as NV12 or RGB planar by ODMA. Platform code is responsible
//! for FDT discovery, clock gates, MMIO mapping, IRQ registration, coherent DMA
//! allocation, waiting, and user ABI translation.

#![no_std]

extern crate alloc;

pub mod hw;
pub mod irq;
pub mod registers;
pub mod types;

pub use hw::{Diagnostics, JobCompletion, VpssControl};
pub use irq::{CompletionState, IrqEvent, IrqHandler, RunState, StatsSnapshot};
pub use registers::{MmioError, MmioRegion, RegisterIo};
pub use types::{
    DestinationFrame, Error, Job, Nv12Frame, Plane, Rect, RgbColor, RgbPlanarFrame, Size,
    SourceFrame, Yuv422PlanarFrame,
};
