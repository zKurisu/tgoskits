use core::{alloc::Layout, fmt::Debug, marker::PhantomData, ptr::NonNull, usize};

use log::{error, trace};
use memory_addr::{is_aligned, MemoryAddr};
pub use memory_addr::{pa, va, PhysAddr, VirtAddr};

use crate::{PagingError, PagingResult};

const TABLE_SIZE: usize = 0x1000;
const LEN: usize = TABLE_SIZE / 8;

#[cfg(target_arch = "aarch64")]
use crate::aarch64::flush_tlb;

/// The page sizes supported by the hardware page table.
#[repr(usize)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PageSize {
    /// Size of 4 kilobytes (2<sup>12</sup> bytes).
    Size4K = 0x1000,
    /// Size of 2 megabytes (2<sup>21</sup> bytes).
    Size2M = 0x20_0000,
    /// Size of 1 gigabytes (2<sup>30</sup> bytes).
    Size1G = 0x4000_0000,
}

impl PageSize {
    /// Whether this page size is considered huge (larger than 4K).
    pub const fn is_block(self) -> bool {
        matches!(self, Self::Size1G | Self::Size2M)
    }
}

impl From<PageSize> for usize {
    #[inline]
    fn from(size: PageSize) -> usize {
        size as usize
    }
}

pub trait VaddrAtTableIndex {
    fn index_of_table(&self, current_level: usize) -> usize;
}

impl VaddrAtTableIndex for VirtAddr {
    fn index_of_table(&self, current_level: usize) -> usize {
        (self.as_usize() >> (12 + (current_level - 1) * 9)) & (LEN - 1)
    }
}

pub trait GenericPTE {
    type Attrs: Debug + Clone + Copy;
    fn new_page(paddr: PhysAddr, attrs: Self::Attrs, is_block: bool) -> Self;
    fn new_table(paddr: PhysAddr) -> Self;
    fn empty() -> Self;
    fn valid(&self) -> bool;
    fn paddr(&self) -> PhysAddr;
    fn is_block(&self) -> bool;
    fn clear_valid(&mut self);
}

/// 必须手动释放
#[derive(Clone, Copy)]
pub struct PageTableRef<PTE: GenericPTE, const LEVLES: usize> {
    root_paddr: PhysAddr,
    _marker: PhantomData<PTE>,
}

impl<PTE: GenericPTE, const LEVELS: usize> PageTableRef<PTE, LEVELS> {
    pub unsafe fn try_new(access: &mut impl Access) -> PagingResult<Self> {
        let addr = Self::alloc_table(access)?;
        Ok(Self {
            root_paddr: access.virt_to_phys(addr).into(),
            _marker: PhantomData,
        })
    }

    pub unsafe fn from_paddr(paddr: PhysAddr) -> Self {
        Self {
            root_paddr: paddr.into(),
            _marker: PhantomData,
        }
    }

    pub fn paddr(&self) -> PhysAddr {
        self.root_paddr
    }

    unsafe fn alloc_table(alloc: &mut impl Access) -> PagingResult<NonNull<[PTE; LEN]>> {
        let layout = Layout::from_size_align_unchecked(TABLE_SIZE, TABLE_SIZE);
        if let Some(addr) = alloc.alloc(layout) {
            addr.write_bytes(0, TABLE_SIZE);
            Ok(addr.cast())
        } else {
            Err(PagingError::NoMemory)
        }
    }

    unsafe fn get_entry_mut_or_create(
        &mut self,
        vaddr: VirtAddr,
        page_size: PageSize,
        access: &mut impl Access,
    ) -> PagingResult<&mut PTE> {
        let mut table = self.table_of_mut(self.root_paddr, access);
        for level in (1..LEVELS + 1).rev() {
            let pte = &mut table[vaddr.index_of_table(level)];
            if page_size as usize == 1 << ((level - 1) * 9 + 12) {
                return Ok(pte);
            }
            table = self.next_table_mut_or_create(pte, access)?;
        }
        unreachable!()
    }
    fn next_table_mut<'a>(
        &self,
        entry: &PTE,
        access: &mut impl Access,
    ) -> PagingResult<&'a mut [PTE]> {
        if !entry.valid() {
            Err(PagingError::NotMapped)
        } else if entry.is_block() {
            Err(PagingError::MappedToHugePage)
        } else {
            Ok(self.table_of_mut(entry.paddr(), access))
        }
    }

    // fn table_of<'a>(&self, paddr: PhysAddr, access: &mut impl Access) -> &'a [PTE] {
    //     let ptr = access.phys_to_virt(paddr.into());
    //     unsafe { core::slice::from_raw_parts(ptr.as_ptr(), LEN) }
    // }

    fn table_of_mut<'a>(&self, paddr: PhysAddr, access: &mut impl Access) -> &'a mut [PTE] {
        let mut ptr = access.phys_to_virt::<PTE>(paddr.into());
        unsafe { core::slice::from_raw_parts_mut(ptr.as_mut(), LEN) }
    }
    unsafe fn next_table_mut_or_create<'a>(
        &mut self,
        entry: &mut PTE,
        access: &mut impl Access,
    ) -> PagingResult<&'a mut [PTE]> {
        if entry.valid() {
            self.next_table_mut(entry, access)
        } else {
            let addr = Self::alloc_table(access)?;
            let paddr = access.virt_to_phys(addr).into();
            *entry = GenericPTE::new_table(paddr);
            Ok(self.table_of_mut(paddr, access))
        }
    }

    /// Maps a virtual page to a physical frame with the given `page_size`
    /// and mapping `flags`.
    ///
    /// The virtual page starts with `vaddr`, amd the physical frame starts with
    /// `target`. If the addresses is not aligned to the page size, they will be
    /// aligned down automatically.
    ///
    /// Returns [`Err(PagingError::AlreadyMapped)`](PagingError::AlreadyMapped)
    /// if the mapping is already present.
    pub unsafe fn map(
        &mut self,
        vaddr: VirtAddr,
        paddr: PhysAddr,
        page_size: PageSize,
        attrs: PTE::Attrs,
        access: &mut impl Access,
    ) -> PagingResult {
        let entry = self.get_entry_mut_or_create(vaddr, page_size, access)?;
        if entry.valid() {
            return Err(PagingError::AlreadyMapped);
        }
        *entry = GenericPTE::new_page(paddr.align_down(page_size), attrs, page_size.is_block());
        Ok(())
    }

    /// Map a contiguous virtual memory region to a contiguous physical memory
    /// region with the given mapping `flags`.
    ///
    /// The virtual and physical memory regions start with `vaddr` and `paddr`
    /// respectively. The region size is `size`. The addresses and `size` must
    /// be aligned to 4K, otherwise it will return [`Err(PagingError::NotAligned)`].
    ///
    /// When `allow_huge` is true, it will try to map the region with huge pages
    /// if possible. Otherwise, it will map the region with 4K pages.
    ///
    /// [`Err(PagingError::NotAligned)`]: PagingError::NotAligned
    pub unsafe fn map_region(
        &mut self,
        vaddr: VirtAddr,
        paddr: PhysAddr,
        size: usize,
        attrs: PTE::Attrs,
        allow_block: bool,
        access: &mut impl Access,
    ) -> PagingResult {
        let align = PageSize::Size4K as usize;

        if !vaddr.is_aligned(align) || !paddr.is_aligned(align) || !is_aligned(size, align) {
            return Err(PagingError::NotAligned);
        }
        trace!(
            "map_region({:#x}): [{:#x}, {:#x}) -> [{:#x}, {:#x}) {:?}",
            self.root_paddr,
            vaddr,
            vaddr + size,
            paddr,
            paddr + size,
            attrs,
        );
        let mut vaddr = vaddr;
        let mut paddr = paddr;
        let mut size = size;
        while size > 0 {
            let page_size = if allow_block {
                if vaddr.is_aligned(PageSize::Size1G)
                    && paddr.is_aligned(PageSize::Size1G)
                    && size >= PageSize::Size1G as usize
                {
                    PageSize::Size1G
                } else if vaddr.is_aligned(PageSize::Size2M)
                    && paddr.is_aligned(PageSize::Size2M)
                    && size >= PageSize::Size2M as usize
                {
                    PageSize::Size2M
                } else {
                    PageSize::Size4K
                }
            } else {
                PageSize::Size4K
            };
            self.map(vaddr, paddr, page_size, attrs, access)
                .inspect_err(|e| {
                    error!(
                        "failed to map page: {:#x?}({:?}) -> {:#x?}, {:?}",
                        vaddr, page_size, paddr, e
                    )
                })?;
            vaddr += page_size as usize;
            paddr += page_size as usize;
            size -= page_size as usize;
        }
        Ok(())
    }
}

pub trait Access {
    unsafe fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>>;
    unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout);
    fn virt_to_phys<T>(&self, addr: NonNull<T>) -> usize;
    fn phys_to_virt<T>(&self, phys: usize) -> NonNull<T>;
}
