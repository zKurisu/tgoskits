//! Shared ownership boundary for the single SG2002 JPU engine.

use ax_memory_addr::PhysAddr;
use dma_api::DmaError;
use sg200x_bsp::soc::TOP_BASE;
use sg200x_jpu::{
    FrameLayout, FrameLayoutError, JpuCreateError, JpuDecodeError, JpuDecoder, JpuMmio, JpuScale,
};

use crate::{StarryError, StarryResult, mm::vm_write_slice, sync::Mutex, task::UserTaskRef};

const JPU_REG_BASE: usize = 0x0b00_0000;
const VC_REG_BASE: usize = 0x0b03_0000;
const REG_MMIO_SIZE: usize = 0x1000;
const TOP_MMIO_SIZE: usize = 0x4000;

#[derive(Clone, Copy, Debug)]
pub(super) struct DecodedJpuFrame {
    pub layout: FrameLayout,
    pub dma_address: u64,
}

#[derive(Default)]
struct JpuState {
    decoder: Option<JpuDecoder>,
    vdec_owned: bool,
    /// Successful decodes since the engine was last constructed.
    decode_count: u32,
}

/// After this many successful decodes the JPU is transparently recycled (drop
/// plus fresh construction) so its internal state — BBC/GBU pointers, FIFOs,
/// error counters — is reinitialized by `hardware_init_at()`.
///
/// The C reference driver calls `JPU_SWReset()` between *every* frame; carrying
/// the reset over this many frames keeps the amortized cost negligible while
/// still bounding how much state can accumulate. Without it the engine wedges
/// after a long run (`[JPU] Timeout!`) and decoding never recovers.
const JPU_RECYCLE_INTERVAL: u32 = 100;

impl JpuState {
    fn decoder(&mut self) -> StarryResult<&mut JpuDecoder> {
        // A poisoned engine answers every later decode with `Poisoned`, so
        // waiting for the periodic recycle would keep failing for up to
        // `JPU_RECYCLE_INTERVAL` frames (and, before the decoder released its
        // ownership on drop, made the rebuild itself fail forever). Drop it
        // eagerly; the rebuild below re-runs `hardware_init_at`, which is what
        // actually clears the wedge.
        let poisoned = self
            .decoder
            .as_ref()
            .is_some_and(JpuDecoder::is_poisoned);
        if poisoned {
            warn!(
                "cvi-jpu: recycling poisoned decoder after {} decodes; next decode re-initializes the engine",
                self.decode_count
            );
            self.decoder = None;
            self.decode_count = 0;
        } else if self.decode_count >= JPU_RECYCLE_INTERVAL {
            info!(
                "cvi-jpu: periodic JPU recycle after {} decodes",
                self.decode_count
            );
            self.decoder = None;
            self.decode_count = 0;
        }
        if self.decoder.is_none() {
            self.decoder = Some(create_decoder()?);
        }
        self.decoder.as_mut().ok_or(StarryError::Io)
    }
}

/// Serializes the one SG2002 JPU between the legacy camera ioctl and VDEC.
pub(super) struct CviJpu {
    state: Mutex<JpuState>,
}

impl CviJpu {
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(JpuState {
                decoder: None,
                vdec_owned: false,
                decode_count: 0,
            }),
        }
    }

    pub fn acquire_vdec(&self) -> StarryResult<()> {
        let mut state = self.state.lock();
        if state.vdec_owned {
            return Err(StarryError::ResourceBusy);
        }
        state.decoder()?;
        state.vdec_owned = true;
        Ok(())
    }

    pub fn release_vdec(&self) {
        self.state.lock().vdec_owned = false;
    }

    pub fn decode_camera_to_user(
        &self,
        current: &UserTaskRef,
        jpeg: &[u8],
        destination: *mut u8,
    ) -> StarryResult<usize> {
        let mut state = self.state.lock();
        if state.vdec_owned {
            return Err(crate::StarryError::ResourceBusy);
        }
        // Count attempts (not just successes): a wedged engine must still be
        // recycled after a bounded number of tries.
        state.decode_count = state.decode_count.saturating_add(1);
        let result = state
            .decoder()?
            .decode(jpeg)
            .map_err(|error| map_decode_error(&error))?;
        vm_write_slice(current, destination, result.yuv_data)?;
        Ok(result.yuv_data.len())
    }

    pub fn decode_vdec(&self, jpeg: &[u8], scale: JpuScale) -> StarryResult<DecodedJpuFrame> {
        let mut state = self.state.lock();
        if !state.vdec_owned {
            return Err(StarryError::InvalidInput);
        }
        state.decode_count = state.decode_count.saturating_add(1);
        let result = state
            .decoder()?
            .decode_scaled(jpeg, scale)
            .map_err(|error| map_decode_error(&error))?;
        Ok(DecodedJpuFrame {
            layout: result.layout,
            dma_address: u64::from(result.yuv_dma_addr),
        })
    }

    pub fn read_vdec_frame(
        &self,
        frame_len: usize,
        offset: usize,
        destination: &mut [u8],
    ) -> StarryResult<usize> {
        let state = self.state.lock();
        if !state.vdec_owned {
            return Err(StarryError::InvalidInput);
        }
        let decoder = state.decoder.as_ref().ok_or(StarryError::Io)?;
        decoder
            .copy_completed_frame(frame_len, offset, destination)
            .map_err(|error| map_decode_error(&error))
    }
}

fn map_mmio(physical: usize, size: usize) -> StarryResult<usize> {
    ax_mm::iomap(PhysAddr::from_usize(physical), size)
        .map(|address| address.as_usize())
        .map_err(|error| {
            warn!("cvi-jpu: failed to map MMIO at {physical:#x}+{size:#x}: {error:?}");
            StarryError::Io
        })
}

fn create_decoder() -> StarryResult<JpuDecoder> {
    let mmio = JpuMmio::new(
        map_mmio(JPU_REG_BASE, REG_MMIO_SIZE)?,
        map_mmio(TOP_BASE, TOP_MMIO_SIZE)?,
        map_mmio(VC_REG_BASE, REG_MMIO_SIZE)?,
    );
    let dma = axklib::dma::device(dma_api::DmaDeviceInfo::new(
        dma_api::DmaDomainId::Direct,
        dma_api::DmaCoherency::NonCoherent,
        dma_api::DmaConstraints::new(u32::MAX as u64),
    ));
    // SAFETY: the mappings above cover the documented JPU, TOP, and VC
    // register spans for the lifetime of this global service. `CviJpu` is the
    // sole accessor and serializes every decode through its mutex.
    unsafe { JpuDecoder::new(mmio, dma) }.map_err(map_create_error)
}

fn map_layout_error(_error: FrameLayoutError) -> StarryError {
    StarryError::OperationNotSupported
}

fn map_create_error(error: JpuCreateError) -> StarryError {
    match error {
        JpuCreateError::AlreadyOwned => StarryError::ResourceBusy,
        JpuCreateError::Initialization(message) => {
            warn!("cvi-jpu: initialization failed: {message}");
            StarryError::Io
        }
    }
}

fn map_decode_error(error: &JpuDecodeError) -> StarryError {
    warn!("cvi-jpu: decode failed: {error}");
    match error {
        JpuDecodeError::Layout(error) => map_layout_error(*error),
        JpuDecodeError::Dma(DmaError::NoMemory) => StarryError::NoMemory,
        JpuDecodeError::Dma(_) => StarryError::Io,
        JpuDecodeError::Timeout => StarryError::TimedOut,
        JpuDecodeError::Poisoned
        | JpuDecodeError::EmptyStream
        | JpuDecodeError::InvalidJpeg(_)
        | JpuDecodeError::BufferInvariant(_)
        | JpuDecodeError::DmaAddress(_)
        | JpuDecodeError::HardwareSetup(_)
        | JpuDecodeError::DecodeFailed => StarryError::Io,
    }
}
