//! CV181x VPSS register layout used by the offline IMG_V -> SC_V1 path.

use core::{
    ptr::NonNull,
    sync::atomic::{Ordering, compiler_fence},
};

pub const MMIO_MIN_SIZE: usize = 0x1_0000;

pub const TOP_CFG0: usize = 0x0000;
pub const TOP_CFG1: usize = 0x0004;
pub const TOP_AXI: usize = 0x0008;
pub const TOP_SHD: usize = 0x0010;
pub const TOP_INTR_MASK: usize = 0x0030;
pub const TOP_INTR_STATUS: usize = 0x0034;
pub const TOP_INTR_ENABLE: usize = 0x0038;
pub const TOP_IMG_CTRL: usize = 0x0040;

pub const IMG_V_BASE: usize = 0x2000;
pub const IMG_CFG: usize = IMG_V_BASE;
pub const IMG_OFFSET: usize = IMG_V_BASE + 0x04;
pub const IMG_SIZE: usize = IMG_V_BASE + 0x08;
pub const IMG_PITCH_Y: usize = IMG_V_BASE + 0x0c;
pub const IMG_PITCH_C: usize = IMG_V_BASE + 0x10;
pub const IMG_SHD: usize = IMG_V_BASE + 0x14;
pub const IMG_ADDR0_L: usize = IMG_V_BASE + 0x24;
pub const IMG_ADDR0_H: usize = IMG_V_BASE + 0x28;
pub const IMG_ADDR1_L: usize = IMG_V_BASE + 0x2c;
pub const IMG_ADDR1_H: usize = IMG_V_BASE + 0x30;
pub const IMG_ADDR2_L: usize = IMG_V_BASE + 0x34;
pub const IMG_ADDR2_H: usize = IMG_V_BASE + 0x38;
pub const IMG_CSC_COEF0: usize = IMG_V_BASE + 0x40;
pub const IMG_CSC_COEF1: usize = IMG_V_BASE + 0x44;
pub const IMG_CSC_COEF2: usize = IMG_V_BASE + 0x48;
pub const IMG_CSC_COEF3: usize = IMG_V_BASE + 0x4c;
pub const IMG_CSC_COEF4: usize = IMG_V_BASE + 0x50;
pub const IMG_CSC_COEF5: usize = IMG_V_BASE + 0x54;
pub const IMG_CSC_SUB: usize = IMG_V_BASE + 0x58;
pub const IMG_CSC_ADD: usize = IMG_V_BASE + 0x5c;
pub const IMG_FIFO_THR: usize = IMG_V_BASE + 0x60;
pub const IMG_DBG: usize = IMG_V_BASE + 0x68;
pub const IMG_AXI_STATUS: usize = IMG_V_BASE + 0x70;

pub const SC_V1_BASE: usize = 0x5000;
pub const SC_CFG: usize = SC_V1_BASE;
pub const SC_SHD: usize = SC_V1_BASE + 0x04;
pub const SC_STATUS: usize = SC_V1_BASE + 0x08;
pub const SC_SRC_SIZE: usize = SC_V1_BASE + 0x0c;
pub const SC_CROP_OFFSET: usize = SC_V1_BASE + 0x10;
pub const SC_CROP_SIZE: usize = SC_V1_BASE + 0x14;
pub const SC_BORDER_CFG: usize = SC_V1_BASE + 0x80;
pub const SC_BORDER_OFFSET: usize = SC_V1_BASE + 0x84;
pub const SC_COEF0: usize = SC_V1_BASE + 0x118;
pub const SC_COEF1: usize = SC_V1_BASE + 0x11c;
pub const SC_COEF2: usize = SC_V1_BASE + 0x120;
pub const SC_SC_CFG: usize = SC_V1_BASE + 0x134;
pub const SC_H_CFG: usize = SC_V1_BASE + 0x138;
pub const SC_V_CFG: usize = SC_V1_BASE + 0x13c;
pub const SC_OUT_SIZE: usize = SC_V1_BASE + 0x140;
pub const SC_INITIAL_PHASE: usize = SC_V1_BASE + 0x148;

pub const ODMA_BASE: usize = SC_V1_BASE + 0x0c00;
pub const ODMA_CFG: usize = ODMA_BASE;
pub const ODMA_ADDR0_L: usize = ODMA_BASE + 0x04;
pub const ODMA_ADDR0_H: usize = ODMA_BASE + 0x08;
pub const ODMA_ADDR1_L: usize = ODMA_BASE + 0x0c;
pub const ODMA_ADDR1_H: usize = ODMA_BASE + 0x10;
pub const ODMA_ADDR2_L: usize = ODMA_BASE + 0x14;
pub const ODMA_ADDR2_H: usize = ODMA_BASE + 0x18;
pub const ODMA_PITCH_Y: usize = ODMA_BASE + 0x1c;
pub const ODMA_PITCH_C: usize = ODMA_BASE + 0x20;
pub const ODMA_OFFSET_X: usize = ODMA_BASE + 0x24;
pub const ODMA_OFFSET_Y: usize = ODMA_BASE + 0x28;
pub const ODMA_WIDTH: usize = ODMA_BASE + 0x2c;
pub const ODMA_HEIGHT: usize = ODMA_BASE + 0x30;
pub const ODMA_DBG: usize = ODMA_BASE + 0x34;

pub const OUT_CSC_BASE: usize = SC_V1_BASE + 0x0d00;
pub const OUT_CSC_ENABLE: usize = OUT_CSC_BASE;
pub const OUT_CSC_COEF0: usize = OUT_CSC_BASE + 0x04;
pub const OUT_CSC_COEF1: usize = OUT_CSC_BASE + 0x08;
pub const OUT_CSC_COEF2: usize = OUT_CSC_BASE + 0x0c;
pub const OUT_CSC_COEF3: usize = OUT_CSC_BASE + 0x10;
pub const OUT_CSC_COEF4: usize = OUT_CSC_BASE + 0x14;
pub const OUT_CSC_OFFSET: usize = OUT_CSC_BASE + 0x18;
pub const OUT_CSC_FRAC0: usize = OUT_CSC_BASE + 0x1c;
pub const OUT_CSC_FRAC1: usize = OUT_CSC_BASE + 0x20;

pub const IRQ_IMG_V_END: u32 = 1 << 5;
pub const IRQ_SC_V1_END: u32 = 1 << 7;
pub const IRQ_PROGRAM_LATE: u32 = 1 << 10;
pub const IRQ_OFFLINE_MASK: u32 = IRQ_IMG_V_END | IRQ_SC_V1_END | IRQ_PROGRAM_LATE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MmioError {
    WindowTooSmall,
}

/// Register access boundary shared by the task-side controller and IRQ side.
pub trait RegisterIo: Clone + Send + Sync + 'static {
    fn read32(&self, offset: usize) -> u32;
    fn write32(&self, offset: usize, value: u32);

    fn update32(&self, offset: usize, mask: u32, value: u32) {
        let old = self.read32(offset);
        self.write32(offset, (old & !mask) | (value & mask));
    }
}

/// A mapped VPSS MMIO window.
#[derive(Clone, Copy, Debug)]
pub struct MmioRegion {
    base: NonNull<u8>,
    size: usize,
}

impl MmioRegion {
    /// Creates a register accessor over an existing device mapping.
    ///
    /// # Safety
    ///
    /// `base..base + size` must remain a valid, device-memory mapping for the
    /// full lifetime of every clone. No other driver may concurrently program
    /// registers owned by this VPSS instance.
    pub unsafe fn new(base: NonNull<u8>, size: usize) -> Result<Self, MmioError> {
        if size < MMIO_MIN_SIZE {
            return Err(MmioError::WindowTooSmall);
        }
        Ok(Self { base, size })
    }

    fn ptr(&self, offset: usize) -> *mut u32 {
        assert!(offset.is_multiple_of(4));
        assert!(offset.checked_add(4).is_some_and(|end| end <= self.size));
        // SAFETY: Bounds and alignment are checked above; constructor safety
        // requires the mapping to remain valid.
        unsafe { self.base.as_ptr().add(offset).cast::<u32>() }
    }
}

impl RegisterIo for MmioRegion {
    fn read32(&self, offset: usize) -> u32 {
        compiler_fence(Ordering::SeqCst);
        // SAFETY: `ptr` validates the offset and the constructor guarantees a
        // live MMIO mapping.
        let value = unsafe { self.ptr(offset).read_volatile() };
        compiler_fence(Ordering::SeqCst);
        value
    }

    fn write32(&self, offset: usize, value: u32) {
        compiler_fence(Ordering::SeqCst);
        // SAFETY: `ptr` validates the offset and the constructor guarantees a
        // live MMIO mapping.
        unsafe { self.ptr(offset).write_volatile(value) };
        compiler_fence(Ordering::SeqCst);
    }
}

// SAFETY: Device access is volatile and callers serialize task-side control;
// the IRQ endpoint owns only the W1C interrupt status operation.
unsafe impl Send for MmioRegion {}
// SAFETY: See `Send`; clones refer to the same stable mapping.
unsafe impl Sync for MmioRegion {}
