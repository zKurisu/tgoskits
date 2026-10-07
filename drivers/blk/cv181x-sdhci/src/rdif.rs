//! RDIF block-device adapter for [`crate::Cv181xSdhci`].

use dma_api::DeviceDma;
pub use rdif_block::{
    BInterface, BIrqHandler, BOwnedQueue, BQueue, BlkError, IQueue, IQueueOwned, Interface,
    OwnedRequest, PollError, QueueHandle, Request, RequestId as RdifRequestId,
    RequestPoll as OwnedRequestPoll, RequestStatus, SubmitError,
};
use sdhci_host::{ADMA2_MAX_BLOCKS, ADMA2_MAX_TRANSFER_SIZE};
pub use sdmmc_protocol::rdif::{config::BlockConfig, device::BlockDevice, queue::BlockQueue};
use sdmmc_protocol::sdio::{card::SdioSdmmc, host2::SdioHost2Adapter};

use crate::Cv181xSdhci;

pub fn device(
    card: SdioSdmmc<SdioHost2Adapter<Cv181xSdhci>>,
    config: BlockConfig,
) -> BlockDevice<SdioHost2Adapter<Cv181xSdhci>> {
    BlockDevice::new(card, config)
}

/// ADMA2-backed block config: multi-block transfers up to the ADMA2 window,
/// matching the limits the underlying [`sdhci_host::Sdhci`] advertises.
pub fn dma_config(
    name: &'static str,
    capacity_blocks: u64,
    irq_driven: bool,
    dma: DeviceDma,
) -> BlockConfig {
    BlockConfig::dma(name, capacity_blocks, irq_driven, dma)
        .with_max_blocks_per_request(ADMA2_MAX_BLOCKS)
        .with_max_segment_size(ADMA2_MAX_TRANSFER_SIZE)
}

pub const fn fifo_config(
    name: &'static str,
    capacity_blocks: u64,
    irq_driven: bool,
) -> BlockConfig {
    BlockConfig::fifo(name, capacity_blocks, irq_driven)
}

#[cfg(test)]
mod tests {
    use sdmmc_protocol::rdif as protocol_rdif;

    use super::*;

    #[test]
    fn fifo_config_is_irq_driven_without_dma() {
        let config = fifo_config("cvsd", 16, true);
        let limits = protocol_rdif::queue_limits(&config, config.dma_mask);

        assert_eq!(config.name, "cvsd");
        assert_eq!(config.capacity_blocks, 16);
        assert!(config.irq_driven);
        assert!(!config.uses_dma());
        assert_eq!(limits.max_blocks_per_request, 1);
        assert_eq!(limits.max_segment_size, protocol_rdif::BLOCK_SIZE);
    }

    #[test]
    fn dma_config_advertises_adma_window() {
        let config = dma_config(
            "cvsd",
            16,
            true,
            dma_api::DeviceDma::new_legacy(u32::MAX as u64, &TEST_DMA),
        );
        let limits = protocol_rdif::queue_limits(&config, config.dma_mask);

        assert!(config.uses_dma());
        assert_eq!(limits.max_blocks_per_request, ADMA2_MAX_BLOCKS);
        assert_eq!(limits.max_segment_size, ADMA2_MAX_TRANSFER_SIZE);
    }

    struct TestDma;
    static TEST_DMA: TestDma = TestDma;

    impl dma_api::DmaOp for TestDma {
        fn page_size(&self) -> usize {
            protocol_rdif::BLOCK_SIZE
        }

        unsafe fn alloc_contiguous(
            &self,
            _constraints: dma_api::DmaConstraints,
            _layout: core::alloc::Layout,
        ) -> Option<dma_api::DmaAllocHandle> {
            None
        }

        unsafe fn dealloc_contiguous(&self, _handle: dma_api::DmaAllocHandle) {}

        unsafe fn alloc_coherent(
            &self,
            _constraints: dma_api::DmaConstraints,
            _layout: core::alloc::Layout,
        ) -> Option<dma_api::DmaAllocHandle> {
            None
        }

        unsafe fn dealloc_coherent(&self, _handle: dma_api::DmaAllocHandle) {}

        unsafe fn map_streaming(
            &self,
            _constraints: dma_api::DmaConstraints,
            _addr: core::ptr::NonNull<u8>,
            _size: core::num::NonZeroUsize,
            _direction: dma_api::DmaDirection,
        ) -> Result<dma_api::DmaMapHandle, dma_api::DmaError> {
            Err(dma_api::DmaError::NoMemory)
        }

        unsafe fn unmap_streaming(&self, _handle: dma_api::DmaMapHandle) {}
    }
}
