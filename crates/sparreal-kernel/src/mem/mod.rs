mod addr;

pub mod dma;
#[cfg(feature = "mmu")]
pub mod mmu;

use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::{null_mut, NonNull},
};

pub use addr::*;
use buddy_system_allocator::Heap;
use log::*;

use crate::{
    kernel::KernelConfig,
    sync::{RwLock, RwLockWriteGuard},
};

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap = LockedHeap::new();

pub const BYTES_1K: usize = 1024;
pub const BYTES_1M: usize = 1024 * BYTES_1K;
pub const BYTES_1G: usize = 1024 * BYTES_1M;

pub unsafe fn init(kconfig: &KernelConfig) {
    #[cfg(feature = "mmu")]
    mmu::set_va_offset(kconfig.boot_info.va_offset);

    let stack_size = kconfig.boot_info.hart_stack_size;
    let start =
        (kconfig.boot_info.main_memory.start + kconfig.boot_info.main_memory_heap_offset).to_virt();
    let size =
        kconfig.boot_info.main_memory.size - kconfig.boot_info.main_memory_heap_offset - stack_size;
    let stack_top = kconfig.stack_top.to_virt();

    debug!("Heap: [{}, {})", start, start + size);
    debug!("Stack: [{}, {})", stack_top - stack_size, stack_top);

    let mut heap = HEAP_ALLOCATOR.write();
    heap.init(start.as_usize(), size);

    debug!("Heap initialized.");

    #[cfg(feature = "mmu")]
    {
        let mut heap_mut = PageAllocatorRef::new(heap);
        if let Err(e) = mmu::init_table(kconfig, &mut heap_mut) {
            error!("Failed to initialize page table: {:?}", e);
        }
    }
}

struct LockedHeap(RwLock<Heap<32>>);

unsafe impl Sync for LockedHeap {}
unsafe impl Send for LockedHeap {}

impl LockedHeap {
    const fn new() -> Self {
        Self(RwLock::new(Heap::new()))
    }

    fn write(&self) -> RwLockWriteGuard<'_, Heap<32>> {
        self.0.write()
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.write().alloc(layout) {
            Ok(ptr) => ptr.as_ptr(),
            Err(_) => null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.write().dealloc(NonNull::new_unchecked(ptr), layout);
    }
}

pub(crate) trait PhysToVirt<T> {
    fn to_virt(self) -> Virt<T>;
}

impl<T> PhysToVirt<T> for Phys<T> {
    fn to_virt(self) -> Virt<T> {
        let a: usize = self.into();

        #[cfg(feature = "mmu")]
        {
            use mmu::va_offset;
            (a + va_offset()).into()
        }

        #[cfg(not(feature = "mmu"))]
        {
            a.into()
        }
    }
}

pub(crate) trait VirtToPhys<T> {
    fn to_phys(self) -> Phys<T>;
}

impl<T> VirtToPhys<T> for Virt<T> {
    fn to_phys(self) -> Phys<T> {
        #[cfg(feature = "mmu")]
        {
            use mmu::va_offset;
            self.convert_to_phys(va_offset())
        }

        #[cfg(not(feature = "mmu"))]
        {
            let a: usize = self.into();
            a.into()
        }
    }
}

#[allow(dead_code)]
pub struct PageAllocatorRef<'a> {
    inner: RwLockWriteGuard<'a, Heap<32>>,
}
impl<'a> PageAllocatorRef<'a> {
    pub fn new(inner: RwLockWriteGuard<'a, Heap<32>>) -> Self {
        Self { inner }
    }
}

#[cfg(feature = "mmu")]
impl page_table_generic::Access for PageAllocatorRef<'_> {
    fn va_offset(&self) -> usize {
        mmu::va_offset()
    }

    unsafe fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        self.inner.alloc(layout).ok()
    }

    unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
        self.inner.dealloc(ptr, layout);
    }
}

#[allow(unused)]
pub struct PageAllocator(Heap<32>);

impl PageAllocator {
    pub unsafe fn new(start: NonNull<u8>, size: usize) -> Self {
        let mut heap = Heap::new();
        heap.init(start.as_ptr() as usize, size);
        Self(heap)
    }
}

#[cfg(feature = "mmu")]
impl page_table_generic::Access for PageAllocator {
    fn va_offset(&self) -> usize {
        mmu::va_offset()
    }

    unsafe fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        self.0.alloc(layout).ok()
    }

    unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
        self.0.dealloc(ptr, layout);
    }
}
