use core::sync::atomic::{Ordering, fence};

use log::*;
use page_table_generic::{Access, PTEArch, PTEGeneric};
use spin::MutexGuard;

use crate::{
    globals::global_val,
    mem::{ALLOCATOR, Align, PhysAddr, VirtAddr},
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
        va_offset()
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
    let info = &global_val().platform_info;
    let debugcon = info.debugcon();

    unsafe {
        let mut access = HeapGuard(ALLOCATOR.inner.lock());

        let mut table = PageTableRef::create_empty(&mut access).unwrap();

        for memory in info.memorys() {
            let size = memory.end - memory.start;
            let vaddr = VirtAddr::from(memory.start);

            trace!(
                "Mapping memory [{}, {}) -> [{}, {})",
                vaddr,
                vaddr + size,
                memory.start,
                memory.end,
            );

            table
                .map_region(
                    MapConfig::new(
                        vaddr.into(),
                        memory.start.into(),
                        AccessSetting::Read | AccessSetting::Write | AccessSetting::Execute,
                        CacheSetting::Normal,
                    ),
                    size,
                    true,
                    &mut access,
                )
                .unwrap()
        }

        if let Some(con) = debugcon {
            let reg = con.addr.align_down(0x1000);

            let vaddr = VirtAddr::from(reg);
            trace!("Mapping stdout {} -> {}", vaddr, reg);

            let _ = table.map_region(
                MapConfig::new(
                    vaddr.into(),
                    reg.into(),
                    AccessSetting::Read | AccessSetting::Write,
                    CacheSetting::Device,
                ),
                0x1000,
                true,
                &mut access,
            );
        }

        fence(Ordering::SeqCst);
        set_kernel_table(table.paddr());
        flush_tlb_all();
    };
}

pub fn iomap(paddr: PhysAddr, size: usize) -> NonNull<u8> {
    unsafe {
        let mut table = get_kernel_table();
        let paddr = paddr.align_down(0x1000);
        let vaddr = VirtAddr::from(paddr);
        let size = size.max(0x1000);

        let mut heap = HeapGuard(ALLOCATOR.inner.lock());

        let _ = table.map_region_with_handle(
            MapConfig::new(
                vaddr.into(),
                paddr.as_usize(),
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

        NonNull::new(vaddr.as_mut_ptr()).unwrap()
    }
}
