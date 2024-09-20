mod addr;
pub mod mmu;

use core::{
    alloc::GlobalAlloc,
    fmt::Display,
    marker::PhantomData,
    ops::DerefMut,
    ptr::{null_mut, NonNull},
};

pub use addr::*;
use buddy_system_allocator::Heap;
use log::*;
use mmu::va_offset;

use crate::{
    driver::device_tree::get_device_tree,
    sync::{RwLock, RwLockReadGuard, RwLockWriteGuard},
    util::boot::{k_boot_debug, k_boot_debug_hex},
    KernelConfig, Platform,
};

#[global_allocator]
pub(crate) static HEAP_ALLOCATOR: LockedHeap = LockedHeap::new();

pub const BYTES_1K: usize = 1024;
pub const BYTES_1M: usize = 1024 * BYTES_1K;
pub const BYTES_1G: usize = 1024 * BYTES_1M;

static mut MEMORY_START: usize = 0;
static mut MEMORY_SIZE: usize = 0;

pub unsafe fn init(kconfig: &KernelConfig) {
    #[cfg(feature = "mmu")]
    mmu::set_va_offset(kconfig.va_offset);

    let stack_size = kconfig.hart_stack_size * kconfig.cpu_count;
    let mut start = (kconfig.main_memory.start + kconfig.main_memory_heap_offset).to_virt();
    let mut size = kconfig.main_memory.size - kconfig.main_memory_heap_offset - stack_size;
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

    fn read(&self) -> RwLockReadGuard<'_, Heap<32>> {
        self.0.read()
    }

    fn write(&self) -> RwLockWriteGuard<'_, Heap<32>> {
        self.0.write()
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        match self.write().alloc(layout) {
            Ok(ptr) => ptr.as_ptr(),
            Err(_) => null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        self.write().dealloc(NonNull::new_unchecked(ptr), layout);
    }
}

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
        let mut start = (cfg.main_memory.start + cfg.main_memory_heap_offset).to_virt();
        let mut size = cfg.main_memory.size - cfg.main_memory_heap_offset - cfg.hart_stack_size;
        let memory_end = (cfg.main_memory.start + cfg.main_memory.size).to_virt();

        debug!(
            "Heap: [{:#x}, {:#x})",
            start.as_usize(),
            start.as_usize() + size
        );
        debug!(
            "Stack: [{:#x}, {:#x})",
            memory_end.as_usize() - cfg.hart_stack_size,
            memory_end.as_usize()
        );

        let mut heap = HEAP_ALLOCATOR.write();
        heap.init(start.as_usize(), size);

        #[cfg(feature = "mmu")]
        {
            let mut heap_mut = AllocatorRef::new(&mut heap);
            mmu::init_page_table::<P>(cfg, &mut heap_mut).unwrap();
        }
    }

    pub fn iomap(&self, addr: PhysAddr, size: usize) -> NonNull<u8> {
        #[cfg(feature = "mmu")]
        let ptr = unsafe { mmu::iomap_bk::<P>(addr, size) };
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

pub struct PageAllocatorRef<'a> {
    inner: RwLockWriteGuard<'a, Heap<32>>,
}
impl<'a> PageAllocatorRef<'a> {
    pub fn new(inner: RwLockWriteGuard<'a, Heap<32>>) -> Self {
        Self { inner }
    }
}

impl page_table_interface::Access for PageAllocatorRef<'_> {
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

pub struct PageAllocator(Heap<32>);

impl PageAllocator {
    pub unsafe fn new(start: NonNull<u8>, size: usize) -> Self {
        let mut heap = Heap::new();
        heap.init(start.as_ptr() as usize, size);
        Self(heap)
    }
}

#[cfg(feature = "mmu")]
impl page_table_interface::Access for PageAllocator {
    fn va_offset(&self) -> usize {
        va_offset()
    }

    unsafe fn alloc(&mut self, layout: core::alloc::Layout) -> Option<usize> {
        match self.0.alloc(layout) {
            Ok(addr) => Some(addr.as_ptr() as usize - va_offset()),
            Err(_) => None,
        }
    }

    unsafe fn dealloc(&mut self, ptr: usize, layout: core::alloc::Layout) {
        self.0.dealloc(
            NonNull::new_unchecked((ptr + va_offset()) as *mut u8),
            layout,
        );
    }
}
