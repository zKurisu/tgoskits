#![no_std]

use core::{alloc::Layout, fmt::Debug, marker::PhantomData, ptr::NonNull};

use log::{error, trace};
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
    fn va_offset(&self) -> usize;
    /// Alloc memory for a page table entry.
    ///
    /// # Safety
    ///
    /// should be deallocated by [`dealloc`].
    unsafe fn alloc(&mut self, layout: Layout) -> Option<PhysAddr>;
    /// dealloc memory for a page table entry.
    ///
    /// # Safety
    ///
    /// ptr must be allocated by [`alloc`].
    unsafe fn dealloc(&mut self, ptr: PhysAddr, layout: Layout);
    fn phys_to_virt<T>(&self, phys: PhysAddr) -> NonNull<T> {
        unsafe { NonNull::new_unchecked((phys.as_usize() + self.va_offset()) as *mut u8) }.cast()
    }
}

#[derive(Debug, Clone)]
pub struct PTEConfig {
    pub paddr: PhysAddr,
    pub is_block: bool,
    pub attributes: PageAttribute,
}

pub trait GenericPTE: Debug + Clone + Copy + Sync + Send + Sized + 'static {
    const PAGE_SIZE: usize = 4096;

    /// Creates a page table entry point to a terminate page or block.
    fn new_page(pte: PTEConfig) -> Self;

    fn new_table(paddr: PhysAddr) -> Self {
        Self::new_page(PTEConfig {
            paddr,
            is_block: false,
            attributes: PageAttribute::Read,
        })
    }

    fn read(&self) -> PTEConfig;

    fn set(&mut self, pte: PTEConfig);

    fn modify<F: Fn(&mut PTEConfig)>(&mut self, f: F) {
        let mut pte = self.read();
        f(&mut pte);
        self.set(pte);
    }

    fn valid(&self) -> bool {
        self.read().attributes.contains(PageAttribute::Read)
    }

    fn paddr(&self) -> PhysAddr {
        self.read().paddr
    }
}

bitflags::bitflags! {
    /// Generic page table entry flags that indicate the corresponding mapped
    /// memory region permissions and attributes.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct PageAttribute: usize {
        const Read = 1;
        const Write = 1 << 2;
        const Execute = 1 << 3;
        const User = 1 << 4;
        const Device = 1 << 5;
        const NonCache = 1 << 6;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MapConfig {
    pub vaddr: VirtAddr,
    pub paddr: PhysAddr,
    pub attrs: PageAttribute,
}
/// A reference to a page table.
///
/// `LEN` is the number of entries in a page table.
/// `LEVEL` is the max level of the page table.
#[derive(Clone, Copy)]
pub struct PageTableRef<'a, P: GenericPTE, const LEN: usize, const LEVEL: usize> {
    addr: PhysAddr,
    // 当前的页表等级
    pub level: usize,
    _marker: PhantomData<&'a P>,
}

impl<'a, P: GenericPTE, const LEN: usize, const LEVEL: usize> PageTableRef<'_, P, LEN, LEVEL> {
    pub fn from_addr(addr: PhysAddr, level: usize) -> Self {
        PageTableRef {
            addr,
            level,
            _marker: PhantomData,
        }
    }
}

impl<'a, P: GenericPTE, const LEN: usize, const LEVEL: usize> PageTableRef<'a, P, LEN, LEVEL> {
    const IDX_POW: usize = log2(LEN);
    const TABLE_SIZE: usize = LEN * size_of::<P>();
    const PTE_PADDR_OFFSET: usize = log2(P::PAGE_SIZE);

    pub fn paddr(&self) -> PhysAddr {
        self.addr
    }

    /// New page table and returns a reference to it.
    ///
    /// `level` is level of this page, should from 1 to up.
    ///
    /// # Panics
    ///
    /// Panics if level is not supported.
    ///
    /// # Errors
    ///
    /// This function will return an error if allocating memory fails.
    ///
    /// # Safety
    ///
    /// table should be deallocated manually.
    pub unsafe fn new(
        level: usize,
        access: &mut impl Access,
    ) -> PagingResult<PageTableRef<'static, P, LEN, LEVEL>> {
        assert!(level > 0);
        let addr = Self::alloc_table(access)?;
        Ok(PageTableRef::from_addr(addr, level))
    }

    pub fn from_ref(value: &'a [P; LEN], level: usize, access: &mut impl Access) -> Self {
        Self::from_addr((value.as_ptr() as usize - access.va_offset()).into(), level)
    }

    fn index_of_table(&self, vaddr: VirtAddr) -> usize {
        (vaddr.as_usize() >> self.entry_size_shift()) & (LEN - 1)
    }
    fn level_entry_size_shift(level: usize) -> usize {
        Self::PTE_PADDR_OFFSET + (level - 1) * Self::IDX_POW
    }

    fn entry_size_shift(&self) -> usize {
        Self::level_entry_size_shift(self.level)
    }

    // 每个页表项的大小
    pub fn entry_size(&self) -> usize {
        1 << self.entry_size_shift()
    }

    fn as_slice<'b: 'a>(&self, access: &impl Access) -> &'b [P; LEN] {
        unsafe { &*((self.addr.as_usize() + access.va_offset()) as *const [P; LEN]) }
    }
    fn as_slice_mut<'b: 'a>(&self, access: &mut impl Access) -> &'b mut [P; LEN] {
        unsafe { &mut *((self.addr.as_usize() + access.va_offset()) as *mut [P; LEN]) }
    }
    fn next_table(&self, index: usize, access: &impl Access) -> Option<Self> {
        let pte = &self.as_slice(access)[index];
        if pte.read().is_block {
            return None;
        }

        if pte.valid() {
            return Some(Self::from_addr(pte.paddr(), self.level - 1));
        } else {
            None
        }
    }

    unsafe fn next_table_or_create(
        &mut self,
        idx: usize,
        cfg: &MapConfig,
        access: &mut impl Access,
    ) -> PagingResult<Self> {
        let pte = &mut self.as_slice_mut(access)[idx];

        if pte.valid() {
            return Ok(Self::from_addr(pte.paddr(), self.level - 1));
        } else {
            let table = Self::new(self.level - 1, access)?;
            let ptr = table.addr;
            pte.modify(|p| {
                p.paddr = ptr;
                p.is_block = false;
                p.attributes = cfg.attrs;
            });

            Ok(table)
        }
    }

    unsafe fn get_entry_mut_or_create(
        &mut self,
        cfg: &MapConfig,
        level: usize,
        access: &mut impl Access,
    ) -> PagingResult<(&mut P, usize)> {
        let mut table = self.clone();
        while table.level > 0 {
            let idx = table.index_of_table(cfg.vaddr);
            let pte = &mut table.as_slice_mut(access)[idx];
            if table.level == level {
                return Ok((pte, table.level));
            }
            table = table.next_table_or_create(idx, cfg, access)?;
        }
        Err(PagingError::NotAligned)
    }

    unsafe fn alloc_table(access: &mut impl Access) -> PagingResult<PhysAddr> {
        let layout = Layout::from_size_align_unchecked(Self::TABLE_SIZE, Self::TABLE_SIZE);
        if let Some(addr) = access.alloc(layout) {
            access
                .phys_to_virt::<u8>(addr)
                .write_bytes(0, Self::TABLE_SIZE);
            Ok(addr)
        } else {
            Err(PagingError::NoMemory)
        }
    }

    pub fn get_pte_mut(&mut self, vaddr: VirtAddr, access: &mut impl Access) -> Option<&mut P> {
        let mut table = self.clone();
        let mut idx;
        let mut this_vaddr = VirtAddr::from(0);

        while table.level > 0 {
            idx = table.index_of_table(vaddr);
            this_vaddr += table.entry_size() * idx;
            let pte = &mut table.as_slice_mut(access)[idx];

            let cfg = pte.read();
            if cfg.is_block {
                return Some(pte);
            }
            if table.level == 1 {
                return Some(pte);
            }

            table = table.next_table(idx, access)?;
        }
        None
    }

    pub fn walk<F: Fn(&WalkInfo<P>)>(&self, f: F, access: &impl Access) {
        self.walk_recursive(0.into(), usize::MAX, Some(&f), None, access);
    }

    fn walk_recursive<F>(
        &self,
        start_vaddr: VirtAddr,
        limit: usize,
        pre_func: Option<&F>,
        post_func: Option<&F>,
        access: &impl Access,
    ) -> Option<()>
    where
        F: Fn(&WalkInfo<P>),
    {
        let start_vaddr_usize: usize = start_vaddr.into();
        let mut n = 0;
        let entries = self.as_slice(access);
        for (i, entry) in entries.iter().enumerate() {
            let vaddr_usize = start_vaddr_usize + i * self.entry_size();
            let vaddr = vaddr_usize.into();

            if entry.valid() {
                let pte = entry.read();
                let is_block = pte.is_block;
                let info = WalkInfo {
                    vaddr,
                    level: self.level,
                    pte,
                    entry: entry.clone(),
                };

                if let Some(func) = pre_func {
                    func(&info);
                }
                if self.level > 1 && !is_block {
                    let table_ref = self.next_table(i, access)?;
                    table_ref.walk_recursive(vaddr, limit, pre_func, post_func, access)?;
                }
                if let Some(func) = post_func {
                    func(&info);
                }
                n += 1;
                if n >= limit {
                    break;
                }
            }
        }
        Some(())
    }
}
pub struct WalkInfo<P: GenericPTE> {
    pub level: usize,
    pub vaddr: VirtAddr,
    pub pte: PTEConfig,
    pub entry: P,
}

impl<P: GenericPTE, const LEN: usize, const LEVEL: usize> PageTableFn
    for PageTableRef<'_, P, LEN, LEVEL>
{
    unsafe fn map(
        &mut self,
        cfg: &MapConfig,
        page_level: usize,
        access: &mut impl Access,
    ) -> PagingResult {
        let align = 1 << Self::level_entry_size_shift(page_level);
        assert!(
            cfg.vaddr.is_aligned(align as usize),
            "vaddr must be aligned to {align:#X}"
        );
        assert!(cfg.paddr.is_aligned_4k(), "paddr must be aligned to 4K");

        let (entry, level) = self.get_entry_mut_or_create(cfg, page_level, access)?;
        if entry.valid() {
            return Err(PagingError::AlreadyMapped);
        }
        entry.set(PTEConfig {
            paddr: cfg.paddr,
            is_block: level > 1,
            attributes: cfg.attrs,
        });

        Ok(())
    }

    unsafe fn new(access: &mut impl Access) -> PagingResult<Self> {
        PageTableRef::new(LEVEL, access)
    }

    fn level_entry_size(&self, level: usize) -> usize {
        1 << Self::level_entry_size_shift(level)
    }
}

pub trait PageTableFn {
    /// Map a page or block of memory.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the memory is valid and that the page table is valid.
    ///
    /// `page_level` range: `[1, MAX_LEVEL]`</br>
    /// 1 => 4K</br>
    /// 2 => 2M</br>
    /// 3 => 1G
    unsafe fn map(
        &mut self,
        cfg: &MapConfig,
        page_level: usize,
        access: &mut impl Access,
    ) -> PagingResult;

    fn level_entry_size(&self, level: usize) -> usize;

    fn detect_page_level(&self, vaddr: VirtAddr, size: usize) -> usize {
        let max_level = 4;
        for level in (0..max_level).rev() {
            let page_size = self.level_entry_size(level);

            if vaddr.is_aligned(page_size) && size >= page_size {
                return level;
            }
        }
        1
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
    unsafe fn map_region(
        &mut self,
        cfg: MapConfig,
        size: usize,
        allow_block: bool,
        access: &mut impl Access,
    ) -> PagingResult {
        let mut vaddr = cfg.vaddr;
        let mut paddr = cfg.paddr;
        let mut size = size;
        trace!(
            "map_region: [{:#x}, {:#x}) -> [{:#x}, {:#x}) {:?}",
            vaddr,
            vaddr + size,
            paddr,
            paddr + size,
            cfg.attrs,
        );
        while size > 0 {
            let page_level = if allow_block {
                self.detect_page_level(vaddr, size)
            } else {
                1
            };
            let page_size = self.level_entry_size(page_level);
            trace!("page_size: {page_size:#X}");
            self.map(
                &MapConfig {
                    vaddr,
                    paddr,
                    attrs: cfg.attrs,
                },
                page_level,
                access,
            )
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

    /// Create a new page table.
    ///
    /// # Safety
    ///
    /// Should be deallocated manually.
    unsafe fn new(access: &mut impl Access) -> PagingResult<Self>
    where
        Self: Sized;
}

const fn log2(value: usize) -> usize {
    assert!(value > 0, "Value must be positive and non-zero");

    let mut v = value;
    let mut result = 0;

    // 计算最高位的位置
    while v > 1 {
        v >>= 1; // 右移一位
        result += 1;
    }

    result
}

#[cfg(test)]
mod test {
    extern crate std;
    use super::*;

    #[test]
    fn test_log2() {
        assert_eq!(log2(512), 9);
        assert_eq!(log2(4096), 12);
    }
}
