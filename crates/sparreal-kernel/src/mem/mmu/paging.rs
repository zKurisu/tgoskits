use core::sync::atomic::{Ordering, fence};

use log::*;
use page_table_generic::{Access, PTEArch, PTEGeneric};
use spin::MutexGuard;

use crate::{
    globals::global_val,
    mem::{ALLOCATOR, PhysAddr, VirtAddr},
};

use super::*;

pub type PageTableRef<'a> = page_table_generic::PageTableRef<'a, PTEImpl>;

#[allow(unused)]
pub(crate) fn get_kernel_table<'a>() -> PageTableRef<'a> {
    let addr = MMUImpl::get_kernel_table();
    let level = table_level();
    PageTableRef::from_addr(addr, level)
}

#[derive(Clone, Copy)]
pub struct PTEImpl;

impl PTEArch for PTEImpl {
    fn page_size() -> usize {
        MMUImpl::page_size()
    }

    fn level() -> usize {
        MMUImpl::table_level()
    }

    fn new_pte(config: PTEGeneric) -> usize {
        MMUImpl::new_pte(config)
    }

    fn read_pte(pte: usize) -> PTEGeneric {
        MMUImpl::read_pte(pte)
    }
}

struct HeapGuard<'a>(MutexGuard<'a, Heap<32>>);

impl Access for HeapGuard<'_> {
    fn va_offset(&self) -> usize {
        RegionKind::Other.va_offset()
    }

    unsafe fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        self.0.alloc(layout).ok()
    }

    unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
        self.0.dealloc(ptr, layout);
    }
}

pub fn init_table() {
    debug!("Initializing page table...");
    let regions = platform::regsions();
    for r in &regions {
        trace!("{:<12}, [{}, {})", r.name(), r.range.start, r.range.end);
    }

    unsafe {
        {
            let mut access = HeapGuard(ALLOCATOR.inner.lock());

            let mut table = PageTableRef::create_empty(&mut access).unwrap();

            for region in &regions {
                let size = region.range.end - region.range.start;
                let vaddr = VirtAddr::from(region.range.start.raw() + region.kind.va_offset());

                table
                    .map_region(
                        MapConfig::new(
                            vaddr.into(),
                            region.range.start.into(),
                            region.access,
                            region.cache,
                        ),
                        size,
                        true,
                        &mut access,
                    )
                    .unwrap();
            }

            drop(access);

            fence(Ordering::SeqCst);
            debug!("Kernel table -> {:#x}", table.paddr());
            set_kernel_table(table.paddr());
        }
        drop(regions);
        flush_tlb_all();
    };
}

pub fn iomap(paddr: PhysAddr, size: usize) -> NonNull<u8> {
    unsafe {
        let mut table = get_kernel_table();
        let paddr = paddr.align_down(0x1000);
        let vaddr = VirtAddr::from(paddr.raw() + RegionKind::Other.va_offset());
        let size = size.max(0x1000);

        let mut heap = HeapGuard(ALLOCATOR.inner.lock());

        let _ = table.map_region_with_handle(
            MapConfig::new(
                vaddr.into(),
                paddr.into(),
                AccessSetting::Read | AccessSetting::Write,
                CacheSetting::Device,
            ),
            size,
            false,
            &mut heap,
            Some(&|p| {
                unsafe { MMUImpl::flush_tlb(p) };
            }),
        );

        NonNull::new(vaddr.into()).unwrap()
    }
}
