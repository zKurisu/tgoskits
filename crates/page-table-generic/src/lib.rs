#![no_std]

use core::fmt::Debug;

mod def;
mod table;

pub use def::*;
pub use table::*;

pub type PagingResult<T = ()> = Result<T, PagingError>;

pub trait FramAllocator: Clone + Copy + Sync + Send + 'static {
    fn alloc_frame(&self) -> Option<PhysAddr>;

    fn dealloc_frame(&self, frame: PhysAddr);

    fn phys_to_virt(&self, paddr: PhysAddr) -> *mut u8;
}

pub trait TableGeneric: Sync + Send + Clone + Copy + 'static {
    type P: PageTableEntry;

    const PAGE_SIZE: usize = 0x1000;
    const LEVEL: usize = 4;
    const VALID_BITS: usize = 12 + Self::LEVEL * 9;
    // 大页最高支持的级别
    const MAX_BLOCK_LEVEL: usize = 3;
    const TABLE_LEN: usize = Self::PAGE_SIZE / core::mem::size_of::<Self::P>();
    fn flush(vaddr: Option<VirtAddr>);
}

pub trait PageTableEntry: Debug + Sync + Send + Clone + Copy + Sized + 'static {
    fn valid(&self) -> bool;
    fn paddr(&self) -> PhysAddr;
    fn set_paddr(&mut self, paddr: PhysAddr);
    fn set_valid(&mut self, valid: bool);
    fn is_huge(&self) -> bool;
    fn set_is_huge(&mut self, b: bool);
}

#[cfg(all(test, not(target_os = "none")))]
mod tests {
    use super::*;
    extern crate std;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
