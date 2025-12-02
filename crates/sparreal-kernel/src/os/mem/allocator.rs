use core::{
    alloc::GlobalAlloc,
    ptr::{NonNull, null_mut},
};

use buddy_system_allocator::Heap;
use page_table_generic::FrameAllocator;
use spin::Mutex;

use crate::{
    hal::al::memory::page_size,
    os::{
        irq::NoIrqGuard,
        mem::address::{PhysAddr, VirtAddr},
    },
};

#[cfg(target_os = "none")]
#[global_allocator]
pub(super) static ALLOCATOR: KAllocator = KAllocator::new();

fn page_layout() -> core::alloc::Layout {
    core::alloc::Layout::from_size_align(page_size(), page_size()).unwrap()
}

#[derive(Clone, Copy)]
pub struct KAlloc;

impl FrameAllocator for KAlloc {
    fn alloc_frame(&self) -> Option<page_table_generic::PhysAddr> {
        ALLOCATOR.lock_heap32().alloc(page_layout()).ok().map(|nn| {
            let virt = VirtAddr::from(nn);
            let phys: PhysAddr = virt.into();
            page_table_generic::PhysAddr::new(phys.raw())
        })
    }

    fn dealloc_frame(&self, frame: page_table_generic::PhysAddr) {
        let phys = PhysAddr::new(frame.raw());
        let virt: VirtAddr = phys.into();
        let ptr = virt.as_mut_ptr();
        let nn = unsafe { NonNull::new_unchecked(ptr) };
        ALLOCATOR.lock_heap32().dealloc(nn, page_layout());
    }

    fn phys_to_virt(&self, paddr: page_table_generic::PhysAddr) -> *mut u8 {
        let phys = PhysAddr::new(paddr.raw());
        let virt: VirtAddr = phys.into();
        virt.as_mut_ptr()
    }
}

pub struct KAllocator {
    frame32: Mutex<Heap<32>>,
    frame64: Mutex<Heap<64>>,
}

impl KAllocator {
    pub const fn new() -> Self {
        Self {
            frame32: Mutex::new(Heap::empty()),
            frame64: Mutex::new(Heap::empty()),
        }
    }

    pub fn add_to_frame(&self, memory: &mut [u8]) {
        let range = memory.as_mut_ptr_range();
        let start = range.start as usize;
        let end = range.end as usize;

        if Self::range_within_u32(start, end) {
            let mut heap32 = self.frame32.lock();
            unsafe { heap32.add_to_heap(start, end) };
        } else {
            let mut heap64 = self.frame64.lock();
            unsafe { heap64.add_to_heap(start, end) };
        }
    }

    pub(crate) fn lock_heap32(&self) -> spin::MutexGuard<'_, Heap<32>> {
        self.frame32.lock()
    }

    pub(crate) fn lock_heap64(&self) -> spin::MutexGuard<'_, Heap<64>> {
        self.frame64.lock()
    }

    // pub(crate) unsafe fn alloc_with_mask(
    //     &self,
    //     layout: core::alloc::Layout,
    //     dma_mask: u64,
    // ) -> *mut u8 {
    //     let guard = NoIrqGuard::new();
    //     let result = if dma_mask <= u32::MAX as u64 {
    //         Self::try_alloc(&self.heap32, layout)
    //     } else {
    //         Self::try_alloc(&self.heap64, layout).or_else(|| Self::try_alloc(&self.heap32, layout))
    //     };
    //     drop(guard);

    //     result.map_or(null_mut(), |ptr| ptr.as_ptr())
    // }

    #[inline]
    fn try_alloc<const BITS: usize>(
        heap: &Mutex<Heap<BITS>>,
        layout: core::alloc::Layout,
    ) -> Option<NonNull<u8>> {
        let mut guard = heap.lock();
        guard.alloc(layout).ok()
    }

    #[inline]
    fn range_within_u32(start: usize, end: usize) -> bool {
        if start >= end {
            return false;
        }

        let last = end - 1;

        let ps = PhysAddr::from(VirtAddr::from(start));
        let pe = PhysAddr::from(VirtAddr::from(last));

        let limit = PhysAddr::from(u32::MAX as usize);
        ps <= limit && pe <= limit
    }

    #[inline]
    fn ptr_in_32bit(ptr: *mut u8) -> bool {
        let phys = PhysAddr::from(VirtAddr::from(ptr as usize));
        phys <= PhysAddr::from(u32::MAX as usize)
    }
}

unsafe impl GlobalAlloc for KAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let guard = NoIrqGuard::new();
        let result = Self::try_alloc(&self.frame64, layout)
            .or_else(|| Self::try_alloc(&self.frame32, layout));
        drop(guard);

        result.map_or(null_mut(), |ptr| ptr.as_ptr())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        let guard = NoIrqGuard::new();
        let nn = unsafe { NonNull::new_unchecked(ptr) };

        if Self::ptr_in_32bit(ptr) {
            self.frame32.lock().dealloc(nn, layout);
        } else {
            self.frame64.lock().dealloc(nn, layout);
        }
        drop(guard);
    }
}
