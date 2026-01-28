use core::{alloc::Layout, cell::UnsafeCell, ops::Range, ptr::NonNull};

use num_align::NumAlign;
use page_table_generic::FrameAllocator;

use crate::mem::page_size;

struct SimpleAllocator {
    start: usize,
    end: usize,
    current: usize, // 当前分配位置
}

impl SimpleAllocator {
    const fn new() -> Self {
        SimpleAllocator {
            start: 0,
            end: 0,
            current: 0,
        }
    }

    unsafe fn init(&mut self, range: Range<usize>) {
        self.start = range.start;
        self.end = range.end;
        self.current = range.start.max(0x40);
    }

    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        unsafe {
            let start = self.current.align_up(layout.align()) as *mut u8;
            let end = start.add(layout.size());
            if end as usize > self.end {
                return core::ptr::null_mut();
            }
            self.current = end as usize;
            start
        }
    }
}

/// 单线程内存分配器
struct Allocator(UnsafeCell<SimpleAllocator>);
unsafe impl Sync for Allocator {}
unsafe impl Send for Allocator {}

static RAM_ALLOC: Allocator = Allocator(UnsafeCell::new(SimpleAllocator::new()));

#[derive(Clone, Copy)]
pub struct Ram;

impl Ram {
    pub fn current(&self) -> *mut u8 {
        unsafe { (*RAM_ALLOC.0.get()).current as _ }
    }

    pub fn alloc(&self, layout: Layout) -> Option<NonNull<u8>> {
        unsafe { NonNull::new((*RAM_ALLOC.0.get()).alloc(layout)) }
    }
}

impl FrameAllocator for Ram {
    fn alloc_frame(&self) -> Option<page_table_generic::PhysAddr> {
        self.alloc(unsafe { Layout::from_size_align_unchecked(page_size(), page_size()) })
            .map(|ptr| (ptr.as_ptr() as usize).into())
    }

    fn dealloc_frame(&self, _frame: page_table_generic::PhysAddr) {}

    fn phys_to_virt(&self, paddr: page_table_generic::PhysAddr) -> *mut u8 {
        super::phys_to_virt(paddr.raw())
    }
}

pub fn init(range: Range<usize>) {
    println!("Initialize RAM allocator: {:#x?}", range);
    unsafe {
        (*RAM_ALLOC.0.get()).init(range);
    }
}

// pub fn current() -> *mut u8 {
//     Ram {}.current() as _
// }

pub fn used_range() -> Range<usize> {
    let start = unsafe { (*RAM_ALLOC.0.get()).start as _ };
    let end = Ram {}.current() as usize;
    start..end.align_up(page_size())
}
