#![no_std]

use core::{alloc::Layout, cell::UnsafeCell, fmt::Debug, marker::PhantomData, ptr::NonNull};

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

pub trait Access: 'static {
    const VA_OFFSET: usize;
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
        unsafe { NonNull::new_unchecked((phys.as_usize() + Self::VA_OFFSET) as *mut u8) }.cast()
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
    /// range: `[1, MAX_LEVEL]`</br>
    /// 1 => 4K</br>
    /// 2 => 2M</br>
    /// 3 => 1G
    pub page_level: usize,
    pub attrs: PageAttribute,
}

pub struct PageTableRef<'a, P: GenericPTE, const LEN: usize, A: Access> {
    addr: PhysAddr,
    pub level: usize,
    access: &'a UnsafeCell<A>,
    _marker: PhantomData<P>,
}

impl<'a, P: GenericPTE, const LEN: usize, A: Access> Clone for PageTableRef<'a, P, LEN, A> {
    fn clone(&self) -> Self {
        Self::from_addr(self.addr, self.level, unsafe { &mut *self.access.get() })
    }
}

impl<'a, P: GenericPTE, const LEN: usize, A: Access> PageTableRef<'_, P, LEN, A> {
    pub fn from_addr(addr: PhysAddr, level: usize, access: &'a mut A) -> Self {
        let t = access as *mut A as *const UnsafeCell<A>;

        PageTableRef {
            addr,
            level,
            // SAFETY: `T` and `UnsafeCell<T>` have the same memory layout
            access: unsafe { &*t },
            _marker: PhantomData,
        }
    }
}

impl<'a, P: GenericPTE, const LEN: usize, A: Access> PageTableRef<'a, P, LEN, A> {
    const IDX_POW: usize = log2(LEN);
    const TABLE_SIZE: usize = LEN * size_of::<P>();
    const PTE_PADDR_OFFSET: usize = log2(P::PAGE_SIZE);

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
        access: &'a mut A,
    ) -> PagingResult<PageTableRef<'static, P, LEN, A>> {
        assert!(level > 0);
        let addr = Self::alloc_table(access)?;
        Ok(PageTableRef::from_addr(addr, level, access))
    }

    pub fn from_ref(value: &'a [P; LEN], level: usize, access: &mut A) -> Self {
        Self::from_addr(
            (value.as_ptr() as usize - A::VA_OFFSET).into(),
            level,
            access,
        )
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

    fn as_slice<'b: 'a>(&self) -> &'b [P; LEN] {
        unsafe { &*((self.addr.as_usize() + A::VA_OFFSET) as *const [P; LEN]) }
    }
    fn as_slice_mut<'b: 'a>(&self) -> &'b mut [P; LEN] {
        unsafe { &mut *((self.addr.as_usize() + A::VA_OFFSET) as *mut [P; LEN]) }
    }
    fn next_table(&self, index: usize) -> Option<Self> {
        let pte = &self.as_slice()[index];
        if pte.read().is_block {
            return None;
        }

        if pte.valid() {
            return Some(Self::from_addr(pte.paddr(), self.level - 1, unsafe {
                &mut *self.access.get()
            }));
        } else {
            None
        }
    }

    unsafe fn next_table_or_create(&mut self, idx: usize) -> PagingResult<Self> {
        let pte = &mut self.as_slice_mut()[idx];

        if pte.valid() {
            return Ok(Self::from_addr(pte.paddr(), self.level - 1, unsafe {
                &mut *self.access.get()
            }));
        } else {
            let table = Self::new(self.level - 1, &mut *self.access.get())?;
            let ptr = table.addr;
            pte.modify(|p| {
                p.paddr = ptr;
                p.is_block = false;
                p.attributes = PageAttribute::Read;
            });

            Ok(table)
        }
    }

    unsafe fn get_entry_mut_or_create(
        &mut self,
        vaddr: VirtAddr,
        level: usize,
    ) -> PagingResult<(&mut P, usize)> {
        let mut table = self.clone();
        while table.level > 0 {
            let idx = table.index_of_table(vaddr);
            let pte = &mut table.as_slice_mut()[idx];
            if table.level == level {
                return Ok((pte, table.level));
            }
            table = table.next_table_or_create(idx)?;
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

    pub fn get_pte_mut(&mut self, vaddr: VirtAddr) -> Option<&mut P> {
        let mut table = self.clone();
        let mut idx;
        let mut this_vaddr = VirtAddr::from(0);

        while table.level > 0 {
            idx = table.index_of_table(vaddr);
            this_vaddr += table.entry_size() * idx;
            let pte = &mut table.as_slice_mut()[idx];

            let cfg = pte.read();
            if cfg.is_block {
                return Some(pte);
            }
            if table.level == 1 {
                return Some(pte);
            }

            table = table.next_table(idx)?;
        }
        None
    }

    pub fn walk<F: Fn(&WalkInfo)>(&self, f: F) {
        self.walk_recursive(0.into(), usize::MAX, Some(&f), None);
    }

    fn walk_recursive<F>(
        &self,
        start_vaddr: VirtAddr,
        limit: usize,
        pre_func: Option<&F>,
        post_func: Option<&F>,
    ) -> Option<()>
    where
        F: Fn(&WalkInfo),
    {
        let start_vaddr_usize: usize = start_vaddr.into();
        let mut n = 0;
        let entries = self.as_slice();
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
                };

                if let Some(func) = pre_func {
                    func(&info);
                }
                if self.level > 1 && !is_block {
                    let table_ref = self.next_table(i)?;
                    table_ref.walk_recursive(vaddr, limit, pre_func, post_func)?;
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

pub struct WalkInfo {
    pub level: usize,
    pub vaddr: VirtAddr,
    pub pte: PTEConfig,
}

impl<P: GenericPTE, const LEN: usize, A: Access> PageTableMap for PageTableRef<'_, P, LEN, A> {
    unsafe fn map(&mut self, cfg: &MapConfig) -> PagingResult {
        let align = 1 << Self::level_entry_size_shift(cfg.page_level);
        assert!(
            cfg.vaddr.is_aligned(align as usize),
            "vaddr must be aligned to {align:#X}"
        );
        assert!(cfg.paddr.is_aligned_4k(), "paddr must be aligned to 4K");

        let (entry, level) = self.get_entry_mut_or_create(cfg.vaddr, cfg.page_level)?;
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
}

pub trait PageTableMap {
    /// Map a page or block of memory.
    ///
    /// # Safety
    ///
    /// The caller must ensure that the memory is valid and that the page table is valid.
    unsafe fn map(&mut self, cfg: &MapConfig) -> PagingResult;

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
    fn map_region<'a>(&mut self, cfg: MapConfig) -> PagingResult {
        Ok(())
    }
}

pub trait PageTableRefFn: Sized + PageTableMap {
    type PTE: GenericPTE;

    /// Create a new page table.
    ///
    /// # Safety
    ///
    /// Should be deallocated manually.
    unsafe fn new(access: &mut impl Access) -> Self;
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
