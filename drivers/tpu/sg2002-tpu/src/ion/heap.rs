//! Ion 堆管理

use alloc::sync::Arc;
use core::alloc::Layout;

use ax_dma::{self, DMAInfo};

use super::{
    error::{IonError, IonResult},
    types::{IonBuffer, IonHeapType},
};

fn dma_range_fits_tpu(start: u64, size: usize) -> bool {
    let Some(last) = size
        .checked_sub(1)
        .and_then(|offset| start.checked_add(offset as u64))
    else {
        return false;
    };
    last <= u64::from(u32::MAX)
}

/// Ion 堆管理器
pub struct IonHeapManager;

impl Default for IonHeapManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::dma_range_fits_tpu;

    #[test]
    fn rejects_dma_range_starting_above_4gib() {
        assert!(!dma_range_fits_tpu(0x1_0000_0000, 4096));
    }

    #[test]
    fn rejects_dma_range_crossing_4gib() {
        assert!(!dma_range_fits_tpu(0xffff_f000, 8192));
    }

    #[test]
    fn accepts_dma_range_below_4gib() {
        assert!(dma_range_fits_tpu(0x1000, 4096));
        assert!(dma_range_fits_tpu(0xffff_f000, 4096));
    }
}

impl IonHeapManager {
    /// 创建新的堆管理器
    pub const fn new() -> Self {
        Self
    }

    /// 从指定堆分配缓冲区
    pub fn alloc_buffer(
        &self,
        size: usize,
        align: usize,
        heap_type: IonHeapType,
    ) -> IonResult<Arc<IonBuffer>> {
        debug!(
            "Allocating Ion buffer: size={}, align={}, heap_type={:?}",
            size, align, heap_type
        );
        // 校验参数
        if size == 0 {
            return Err(IonError::InvalidArg);
        }

        let dma_info = match heap_type {
            IonHeapType::System => {
                // 系统堆使用普通的 DMA 内存
                self.alloc_dma_buffer(size, align)?
            }
            IonHeapType::DmaCoherent => {
                // DMA coherent 堆
                self.alloc_dma_buffer(size, align)?
            }
            IonHeapType::Carveout => {
                // Carveout 堆暂时不支持，使用 DMA 内存代替
                warn!("Carveout heap not implemented, using DMA heap instead");
                self.alloc_dma_buffer(size, align)?
            }
        };

        let buffer = Arc::new(IonBuffer::new(dma_info, size));
        debug!("Allocated Ion buffer with handle: {:?}", buffer.handle);

        Ok(buffer)
    }

    /// 分配 DMA 内存
    fn alloc_dma_buffer(&self, size: usize, align: usize) -> IonResult<DMAInfo> {
        let layout = Layout::from_size_align(size, align).map_err(|_| IonError::InvalidArg)?;
        // SG2002 multimedia engines and TPU TDMA program raw 32-bit physical
        // addresses without an IOMMU.  A generic coherent allocation may sit
        // above 4 GiB and would then be silently truncated in the hardware
        // array-base registers.
        let dma =
            unsafe { ax_dma::alloc_coherent_pages_dma32(layout).map_err(|_| IonError::NoMemory)? };
        if !dma_range_fits_tpu(dma.bus_addr.as_u64(), size) {
            error!(
                "ION DMA32 allocator returned unreachable range: paddr=0x{:x}, size={}",
                dma.bus_addr.as_u64(),
                size
            );
            unsafe { ax_dma::dealloc_coherent_pages(dma, layout) };
            return Err(IonError::NoMemory);
        }
        Ok(dma)
    }
}
