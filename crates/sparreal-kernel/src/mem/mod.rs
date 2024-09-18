mod addr;
pub mod mmu;

use core::{fmt::Display, marker::PhantomData, ops::DerefMut, ptr::NonNull};

pub use addr::*;
use buddy_system_allocator::{Heap, LockedHeap};
use log::debug;
use mmu::va_offset;

use crate::{
    driver::device_tree::get_device_tree,
    util::boot::{k_boot_debug, k_boot_debug_hex},
    KernelConfig, Platform,
};

#[global_allocator]
pub(crate) static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::empty();

pub const BYTES_1K: usize = 1024;
pub const BYTES_1M: usize = 1024 * BYTES_1K;
pub const BYTES_1G: usize = 1024 * BYTES_1M;

static mut MEMORY_START: usize = 0;
static mut MEMORY_SIZE: usize = 0;

pub(crate) trait VirtToPhys {
    fn to_phys(&self) -> PhysAddr;
}

impl<T> VirtToPhys for NonNull<T> {
    fn to_phys(&self) -> PhysAddr {
        (self.as_ptr() as usize - va_offset()).into()
    }
}

pub(crate) trait PhysToVirt<T> {
    fn to_virt(self) -> Virt<T>;
}

impl<T> PhysToVirt<T> for Phys<T> {
    fn to_virt(self) -> Virt<T> {
        let a: usize = self.into();
        (a + va_offset()).into()
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
        let mut start = (cfg.memory_start + cfg.memory_used).to_virt();
        let mut size = cfg.memory_size - cfg.memory_used - cfg.stack_size;

        debug!(
            "Heap: [{:#x}, {:#x})",
            start.as_usize(),
            start.as_usize() + size
        );
        debug!(
            "Stack: [{:#x}, {:#x})",
            cfg.stack_bottom.as_usize(),
            cfg.stack_bottom.as_usize() + cfg.stack_size
        );

        let mut heap = HEAP_ALLOCATOR.lock();
        heap.init(start.as_usize(), size);

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

impl<P: Platform> Display for MemoryManager<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        unsafe {
            write!(
                f,
                "Primary Memory: @{:#X} ({:#X} bytes)",
                MEMORY_START, MEMORY_SIZE
            )
        }
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

    unsafe fn alloc(&mut self, layout: core::alloc::Layout) -> Option<usize> {
        match self.inner.alloc(layout) {
            Ok(addr) => Some(addr.as_ptr() as usize - va_offset()),
            Err(_) => None,
        }
    }

    unsafe fn dealloc(&mut self, ptr: usize, layout: core::alloc::Layout) {
        self.inner.dealloc(
            NonNull::new_unchecked((ptr + va_offset()) as *mut u8),
            layout,
        );
    }
}
