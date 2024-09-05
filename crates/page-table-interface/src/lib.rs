#![no_std]

use core::{alloc::Layout, ptr::NonNull};

pub use memory_addr::*;

/// The error type for page table operation failures.
#[derive(Debug, PartialEq)]
pub enum PagingError {
    /// Cannot allocate memory.
    NoMemory,
    /// The address is not aligned to the page size.
    NotAligned,
    /// The mapping is not present.
    NotMapped,
    /// The mapping is already present.
    AlreadyMapped,
    /// The page table entry represents a huge page, but the target physical
    /// frame is 4K in size.
    MappedToHugePage,
}

/// The specialized `Result` type for page table operations.
pub type PagingResult<T = ()> = Result<T, PagingError>;

pub trait Access {
    unsafe fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>>;
    unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout);
    fn virt_to_phys<T>(&self, addr: NonNull<T>) -> usize;
    fn phys_to_virt<T>(&self, phys: usize) -> NonNull<T>;
}
pub trait PageTableEntry: Copy + Clone {}

pub enum PageAttribute {
    Read,
    Write,
    Execute,
    Device,
    NonCache,
}

pub trait PageTable {
    unsafe fn new(access: &mut impl Access) -> Self;

    unsafe fn map(
        &mut self,
        vaddr: VirtAddr,
        paddr: PhysAddr,
        page_size: usize,
        attrs: impl Iterator<Item = PageAttribute>,
        access: &mut impl Access,
    ) -> PagingResult;
}
