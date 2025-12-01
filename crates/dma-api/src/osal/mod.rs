use crate::{DmaHandle, Osal};

cfg_if::cfg_if! {
    if #[cfg(target_arch = "aarch64")] {
        #[path = "aarch64.rs"]
        pub mod arch;
    } else{
        #[path = "nop.rs"]
        pub mod arch;
    }
}

pub struct NopOsal;

#[allow(unused_variables)]
impl Osal for NopOsal {
    fn map(
        &self,
        addr: core::ptr::NonNull<u8>,
        size: usize,
        direction: crate::Direction,
    ) -> DmaHandle {
        unimplemented!()
    }

    fn page_size(&self) -> usize {
        unimplemented!()
    }

    fn unmap(&self, _h: DmaHandle) {
        unimplemented!()
    }

    fn flush(&self, addr: core::ptr::NonNull<u8>, size: usize) {
        unimplemented!()
    }

    fn invalidate(&self, addr: core::ptr::NonNull<u8>, size: usize) {
        unimplemented!()
    }

    #[cfg(feature = "alloc")]
    unsafe fn alloc(&self, _dma_mask: u64, _layout: core::alloc::Layout) -> DmaHandle {
        unimplemented!()
    }

    #[cfg(feature = "alloc")]
    unsafe fn dealloc(&self, _h: DmaHandle) {
        unimplemented!()
    }
}
