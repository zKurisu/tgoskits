use core::{
    alloc::GlobalAlloc,
    ptr::{NonNull, null_mut, slice_from_raw_parts_mut},
};

use buddy_system_allocator::Heap;
use log::debug;
use spin::Mutex;

use crate::{globals::global_val, platform::kstack_size, println};

mod addr;
#[cfg(feature = "mmu")]
pub mod mmu;
pub mod region;
pub use addr::*;

#[global_allocator]
static ALLOCATOR: KAllocator = KAllocator {
    inner: Mutex::new(Heap::empty()),
};

pub struct KAllocator {
    pub(crate) inner: Mutex<Heap<32>>,
}

impl KAllocator {
    pub fn reset(&self, memory: &mut [u8]) {
        let mut g = self.inner.lock();

        let mut h = Heap::empty();

        unsafe { h.init(memory.as_mut_ptr() as usize, memory.len()) };

        *g = h;
    }

    pub fn add_to_heap(&self, memory: &mut [u8]) {
        let mut g = self.inner.lock();
        let range = memory.as_mut_ptr_range();

        unsafe { g.add_to_heap(range.start as usize, range.end as usize) };
    }
}

unsafe impl GlobalAlloc for KAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        if let Ok(p) = self.inner.lock().alloc(layout) {
            p.as_ptr()
        } else {
            null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        self.inner
            .lock()
            .dealloc(unsafe { NonNull::new_unchecked(ptr) }, layout);
    }
}

static mut VA_OFFSET: usize = 0;
static mut VA_OFFSET_NOW: usize = 0;

pub(crate) fn set_va_offset(offset: usize) {
    unsafe { VA_OFFSET = offset };
}

pub fn va_offset() -> usize {
    unsafe { VA_OFFSET }
}

pub(crate) unsafe fn set_va_offset_now(va: usize) {
    unsafe { VA_OFFSET_NOW = va };
}

fn va_offset_now() -> usize {
    unsafe { VA_OFFSET_NOW }
}

pub(crate) fn init_heap() {
    let main = global_val().main_memory.clone();
    let mut start = VirtAddr::from(main.start);
    let mut end = VirtAddr::from(main.end);

    let bss_end = crate::mem::region::bss().as_ptr_range().end.into();

    if (start..end).contains(&bss_end) {
        start = bss_end;
    }

    let stack_top = VirtAddr::from(global_val().kstack_top);
    let stack_bottom = stack_top - kstack_size();

    if (start..end).contains(&stack_bottom) {
        end = stack_bottom;
    }

    println!("heap add memory [{}, {})", start, end);
    ALLOCATOR
        .add_to_heap(unsafe { &mut *slice_from_raw_parts_mut(start.as_mut_ptr(), end - start) });

    println!("heap initialized");
}

pub(crate) fn init_page_and_memory() {
    #[cfg(feature = "mmu")]
    mmu::init_table();

    let main = global_val().main_memory.clone();

    for memory in global_val().platform_info.memorys() {
        if memory.contains(&main.start) {
            continue;
        }
        let start = VirtAddr::from(memory.start);
        let end = VirtAddr::from(memory.end);
        let len = memory.end - memory.start;

        debug!("Heap add memory [{}, {})", start, end);
        ALLOCATOR.add_to_heap(unsafe { &mut *slice_from_raw_parts_mut(start.as_mut_ptr(), len) });
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CMemRange {
    pub start: usize,
    pub end: usize,
}

impl CMemRange {
    pub fn as_slice(&self) -> &'static [u8] {
        unsafe { core::slice::from_raw_parts(self.start as *const u8, self.end - self.start) }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct KernelRegions {
    pub text: CMemRange,
    pub rodata: CMemRange,
    pub data: CMemRange,
    pub bss: CMemRange,
}

pub fn iomap(paddr: PhysAddr, size: usize) -> NonNull<u8> {
    if cfg!(feature = "mmu") {
        mmu::iomap(paddr, size)
    } else {
        unsafe { NonNull::new_unchecked(paddr.as_usize() as *mut u8) }
    }
}
