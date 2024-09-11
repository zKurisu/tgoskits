use core::{
    alloc::Layout,
    ptr::{slice_from_raw_parts, slice_from_raw_parts_mut, NonNull},
};

use flat_device_tree::Fdt;
pub use page_table_interface::*;

use crate::{
    driver::device_tree::{get_device_tree, set_dtb_addr},
    Platform,
};

use super::{AllocatorRef, PhysToVirt, BYTES_1G, BYTES_1M, HEAP_ALLOCATOR};

static mut VA_OFFSET: usize = 0;
static mut HEAP_BEGIN_LMA: NonNull<u8> = NonNull::dangling();
static mut MEMORY_START: usize = 0;
static mut MEMORY_SIZE: usize = 0;

pub fn va_offset() -> usize {
    unsafe { VA_OFFSET }
}

pub unsafe fn boot_init<T: PageTableFn>(
    va_offset: usize,
    dtb_addr: NonNull<u8>,
    heap_begin_lma: NonNull<u8>,
    kernel_lma: NonNull<u8>,
) -> PagingResult<T> {
    VA_OFFSET = va_offset;
    let phys_dtb_addr = protect_dtb(dtb_addr, heap_begin_lma);

    set_dtb_addr(phys_dtb_addr);

    let kernel_p = VirtAddr::from(kernel_lma.as_ptr() as usize);
    let virt_equal = kernel_p.align_down(BYTES_1G);

    let mut boot_map_info = BootMapInfo {
        virt: virt_equal + va_offset,
        virt_equal,
        phys: PhysAddr::from(virt_equal.as_usize()),
        size: BYTES_1G,
        heap_start: heap_begin_lma.add(BYTES_1M),
        heap_size: BYTES_1M * 2,
    };

    if let Some(info) = read_dev_tree_boot_map_info(va_offset) {
        boot_map_info = info;
    }

    set_dtb_addr(phys_dtb_addr.map(|p| p.add(va_offset)));

    let mut access = BeforeMMUPageAllocator::new(
        boot_map_info.heap_start.as_ptr() as usize,
        boot_map_info.heap_size,
    );

    MEMORY_START = boot_map_info.phys.into();
    MEMORY_SIZE = boot_map_info.size;

    let mut table = T::new(&mut access)?;

    table.map_region(
        MapConfig {
            vaddr: boot_map_info.virt,
            paddr: boot_map_info.phys,
            attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
        },
        boot_map_info.size,
        true,
        &mut access,
        &|_| {},
    )?;

    // 恒等映射，用于mmu启动过程
    table.map_region(
        MapConfig {
            vaddr: boot_map_info.virt_equal,
            paddr: boot_map_info.phys,
            attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
        },
        boot_map_info.size,
        true,
        &mut access,
        &|_| {},
    )?;

    Ok(table)
}

unsafe fn read_dev_tree_boot_map_info(va_offset: usize) -> Option<BootMapInfo> {
    let fdt = get_device_tree()?;

    let memory = fdt.memory().ok()?;
    let primory = memory.regions().next()?;
    let memory_begin = NonNull::new_unchecked(primory.starting_address as *mut u8);
    let memory_size = primory.size?;
    let heap_size = memory_size / 2;
    let heap_start = memory_begin.add(heap_size);

    let virt_equal = (memory_begin.as_ptr() as usize).into();

    Some(BootMapInfo {
        virt: virt_equal + va_offset,
        virt_equal,
        phys: virt_equal.as_usize().into(),
        size: memory_size,
        heap_start,
        heap_size,
    })
}

struct BootMapInfo {
    heap_start: NonNull<u8>,
    heap_size: usize,
    virt: VirtAddr,
    virt_equal: VirtAddr,
    phys: PhysAddr,
    size: usize,
}

unsafe fn protect_dtb(dtb_addr: NonNull<u8>, mut heap_lma: NonNull<u8>) -> Option<NonNull<u8>> {
    HEAP_BEGIN_LMA = heap_lma;
    let fdt = Fdt::from_ptr(dtb_addr.as_ptr()).ok()?;
    let size = fdt.total_size();
    HEAP_BEGIN_LMA = heap_lma.add(size);
    let dest = &mut *slice_from_raw_parts_mut(heap_lma.as_mut(), size);
    let src = &*slice_from_raw_parts(dtb_addr.as_ptr(), size);
    dest.copy_from_slice(src);
    Some(NonNull::new_unchecked(dest.as_mut_ptr()))
}

struct BeforeMMUPageAllocator {
    end: usize,
    iter: usize,
}

impl BeforeMMUPageAllocator {
    unsafe fn new(start: usize, size: usize) -> Self {
        Self {
            iter: start,
            end: start + size,
        }
    }
}

impl Access for BeforeMMUPageAllocator {
    unsafe fn alloc(&mut self, layout: Layout) -> Option<PhysAddr> {
        let size = layout.size();
        if self.iter + size > self.end {
            return None;
        }
        let ptr = self.iter;
        self.iter += size;
        Some(ptr.into())
    }

    unsafe fn dealloc(&mut self, _ptr: PhysAddr, _layout: Layout) {}

    fn va_offset(&self) -> usize {
        0
    }
}

pub(crate) unsafe fn init_page_table<P: Platform>(
    access: &mut impl Access,
) -> Result<(), PagingError> {
    let mut table = P::Table::new(access)?;
    let vaddr = (MEMORY_START + va_offset()).into();
    let paddr = MEMORY_START.into();
    let size = MEMORY_SIZE;

    table.map_region(
        MapConfig {
            vaddr,
            paddr,
            attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
        },
        size,
        true,
        access,
        &|_| {},
    )?;

    P::set_kernel_page_table(table);
    P::set_user_page_table(None);

    Ok(())
}

pub(crate) unsafe fn iomap<P: Platform>(paddr: PhysAddr, size: usize) -> NonNull<u8> {
    let mut table = P::get_kernel_page_table();
    let vaddr = paddr.to_virt();
    let mut heap = HEAP_ALLOCATOR.lock();
    let mut heap_mut = AllocatorRef::new(&mut heap);
    let vptr = NonNull::new_unchecked(vaddr.as_mut_ptr());

    let _ = table.map_region(
        MapConfig {
            vaddr,
            paddr,
            attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Device,
        },
        size,
        true,
        &mut heap_mut,
        &|addr| {
            P::flush_tlb(Some(addr));
        },
    );
    vptr
}
