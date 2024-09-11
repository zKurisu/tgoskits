pub mod mmu;

use core::{
    marker::PhantomData,
    ops::DerefMut,
    ptr::{null_mut, NonNull},
};

use buddy_system_allocator::{Heap, LockedHeap};
use lock_api::MutexGuard;
use memory_addr::{PhysAddr, VirtAddr};
use mmu::va_offset;

use crate::{
    driver::device_tree::{self, get_device_tree},
    sync::RwLock,
    KernelConfig, Platform,
};

#[global_allocator]
pub(crate) static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::empty();

pub const BYTES_1K: usize = 1024;
pub const BYTES_1M: usize = 1024 * BYTES_1K;
pub const BYTES_1G: usize = 1024 * BYTES_1M;

pub(crate) trait VirtToPhys {
    fn to_phys(&self) -> PhysAddr;
}

impl<T> VirtToPhys for NonNull<T> {
    fn to_phys(&self) -> PhysAddr {
        (self.as_ptr() as usize - va_offset()).into()
    }
}

pub(crate) trait PhysToVirt {
    fn to_virt(&self) -> VirtAddr;
}

impl PhysToVirt for PhysAddr {
    fn to_virt(&self) -> VirtAddr {
        (self.as_usize() + va_offset()).into()
    }
}

pub struct MemoryManager<P: Platform> {
    _marker: PhantomData<P>,
}

impl<P: Platform> Clone for MemoryManager<P> {
    fn clone(&self) -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<P: Platform> MemoryManager<P> {
    pub const fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    pub unsafe fn init(&self, cfg: &KernelConfig) {
        let mut start = cfg.heap_start;
        let mut size = 2 * BYTES_1M;

        if let Some(fdt) = get_device_tree() {
            start = start.add(fdt.total_size());

            if let Some(memory) = fdt.memory().ok() {
                for region in memory.regions() {
                    let used = start.to_phys().as_usize() - region.starting_address as usize;

                    if let Some(mem_size) = region.size {
                        size = mem_size - used;
                    }
                }
            }
        }
        let mut heap = HEAP_ALLOCATOR.lock();
        heap.init(start.as_ptr() as usize, size);

        #[cfg(feature = "mmu")]
        {
            let mut heap_mut = AllocatorRef::new(&mut heap);
            mmu::init_page_table::<P>(&mut heap_mut).unwrap();
        }
    }

    pub fn iomap(&self, addr: PhysAddr, size: usize) -> NonNull<u8> {
        #[cfg(feature = "mmu")]
        let ptr = unsafe { mmu::iomap::<P>(addr, size) };
        #[cfg(not(feature = "mmu"))]
        let ptr = NonNull::new(addr.as_usize() as *mut u8).unwrap();
        ptr
    }
}

pub struct AllocatorRef<'a, G>
where
    G: DerefMut<Target = Heap<32>>,
{
    inner: &'a mut G,
}

impl<'a, G> AllocatorRef<'a, G>
where
    G: DerefMut<Target = Heap<32>>,
{
    fn new(inner: &'a mut G) -> Self {
        Self { inner }
    }
}

#[cfg(feature = "mmu")]
impl<'a, G> page_table_interface::Access for AllocatorRef<'a, G>
where
    G: DerefMut<Target = Heap<32>>,
{
    fn va_offset(&self) -> usize {
        va_offset()
    }

    unsafe fn alloc(&mut self, layout: core::alloc::Layout) -> Option<PhysAddr> {
        match self.inner.alloc(layout) {
            Ok(addr) => Some((addr.as_ptr() as usize - va_offset()).into()),
            Err(_) => None,
        }
    }

    unsafe fn dealloc(&mut self, ptr: PhysAddr, layout: core::alloc::Layout) {
        self.inner.dealloc(
            NonNull::new_unchecked((ptr.as_usize() + va_offset()) as *mut u8),
            layout,
        );
    }
}
