use core::{
    alloc::Layout,
    ptr::{slice_from_raw_parts, slice_from_raw_parts_mut, NonNull},
};

use super::*;
use flat_device_tree::Fdt;
use memory_addr::MemoryAddr;
pub use page_table_interface::*;

use crate::{
    driver::device_tree::{get_device_tree, set_dtb_addr},
    kernel,
    util::{
        self,
        boot::{k_boot_debug, k_boot_debug_hex},
    },
    Platform,
};

struct BootInfo {
    va_offset: usize,
    reserved_start: usize,
    reserved_end: usize,
}

#[link_section = ".data.boot"]
static mut BOOT_INFO: BootInfo = BootInfo {
    va_offset: 0,
    reserved_start: 0,
    reserved_end: 0,
};

fn reserved_start() -> Phys<u8> {
    unsafe { BOOT_INFO.reserved_start.into() }
}
fn reserved_end() -> Phys<u8> {
    unsafe { BOOT_INFO.reserved_end.into() }
}

pub fn va_offset() -> usize {
    unsafe { BOOT_INFO.va_offset }
}

pub unsafe fn boot_init<P: Platform>(
    va_offset: usize,
    dtb_addr: NonNull<u8>,
    kernel_lma: NonNull<u8>,
    kernel_size: usize,
) -> PagingResult<P::Table> {
    BOOT_INFO.va_offset = va_offset;
    BOOT_INFO.reserved_start = kernel_lma.as_ptr() as usize;
    BOOT_INFO.reserved_end = kernel_lma.as_ptr() as usize + kernel_size;

    k_boot_debug::<P>("boot table init\r\n");

    let phys_dtb_addr = protect_dtb(
        dtb_addr,
        NonNull::new_unchecked(BOOT_INFO.reserved_end as _),
    );

    if let Some(addr) = phys_dtb_addr {
        k_boot_debug::<P>("dtb moved to ");
        k_boot_debug_hex::<P>(addr.as_ptr() as usize as _);
        k_boot_debug::<P>("\r\n");
    }

    let stdout = phys_dtb_addr.and_then(|addr| util::boot::stdout_reg(addr));

    set_dtb_addr(phys_dtb_addr);
    DtbPrint::<P>::print();

    let primory_phys_start = reserved_start().align_down(BYTES_1M * 2);
    let primory_virt_start = primory_phys_start.to_virt();
    let primory_virt_start_eq = Virt::<u8>::from(primory_phys_start.as_usize());
    let size = BYTES_1G;

    let heap_start = reserved_end().align_down(0x1000) + BYTES_1M * 2;
    let heep_size = BYTES_1M * 2;

    k_boot_debug::<P>("heap [");
    k_boot_debug_hex::<P>(heap_start.as_usize() as _);
    k_boot_debug::<P>(", ");
    k_boot_debug_hex::<P>((heap_start.as_usize() + heep_size) as _);
    k_boot_debug::<P>(")\r\n");

    let mut access = BeforeMMUPageAllocator::new(heap_start.into(), heep_size);

    let mut table = P::Table::new(&mut access)?;

    k_boot_debug::<P>("new table\r\n");

    let _ = table.map_region(
        MapConfig {
            vaddr: primory_virt_start.into(),
            paddr: primory_phys_start.into(),
            attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
        },
        size,
        true,
        &mut access,
    );
    k_boot_debug::<P>("map @");
    k_boot_debug_hex::<P>(primory_virt_start.as_usize() as _);
    k_boot_debug::<P>("-> @");
    k_boot_debug_hex::<P>(primory_phys_start.as_usize() as _);

    k_boot_debug::<P>(" size ");
    k_boot_debug_hex::<P>(size as _);
    k_boot_debug::<P>("\r\n");
    // 恒等映射，用于mmu启动过程
    let _ = table.map_region(
        MapConfig {
            vaddr: primory_virt_start_eq.into(),
            paddr: primory_phys_start.into(),
            attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
        },
        size,
        true,
        &mut access,
    );

    if let Some(stdout) = stdout {
        let mut reg_virt_eq = Virt::from(stdout.reg);
        let reg_virt = reg_virt_eq + va_offset;
        let reg_phys = reg_virt_eq.convert_to_phys(0);
        k_boot_debug::<P>("map stdout @");
        k_boot_debug_hex::<P>(reg_virt.as_usize() as _);
        k_boot_debug::<P>(" -> ");
        k_boot_debug_hex::<P>(reg_phys.as_usize() as _);
        k_boot_debug::<P>("\r\n");

        table.map_region(
            MapConfig {
                vaddr: reg_virt.into(),
                paddr: reg_phys.into(),
                attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Device,
            },
            stdout.size.max(0x1000),
            true,
            &mut access,
        );
    };

    Ok(table)
}

unsafe fn read_dev_tree_boot_map_info(va_offset: usize) -> Option<BootMapInfo> {
    let fdt = get_device_tree()?;

    let memory = fdt.memory().ok()?;
    let primory = memory.regions().next()?;
    let memory_begin = primory.starting_address;

    let memory_size = primory.size?;
    let heap_size = memory_size / 2;
    let heap_start = NonNull::new_unchecked(memory_begin.add(heap_size) as *mut u8);

    let virt_equal = memory_begin.into();

    Some(BootMapInfo {
        virt: virt_equal + va_offset,
        virt_equal,
        phys: virt_equal.convert_to_phys(0),
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

unsafe fn protect_dtb(dtb_addr: NonNull<u8>, mut kernel_end: NonNull<u8>) -> Option<NonNull<u8>> {
    let fdt = Fdt::from_ptr(dtb_addr.as_ptr()).ok()?;
    let size = fdt.total_size();
    BOOT_INFO.reserved_end += size;
    let dest = &mut *slice_from_raw_parts_mut(kernel_end.as_mut(), size);
    let src = &*slice_from_raw_parts(dtb_addr.as_ptr(), size);
    dest.copy_from_slice(src);
    Some(NonNull::new_unchecked(dest.as_mut_ptr()))
}

struct DtbPrint<P: Platform> {
    _marker: PhantomData<P>,
}

impl<P: Platform> DtbPrint<P> {
    fn print() -> Option<()> {
        let fdt = get_device_tree()?;

        for memory in fdt.memory_reservations() {
            k_boot_debug::<P>("memory reservation: ");
            k_boot_debug_hex::<P>(memory.address() as usize as _);
            k_boot_debug::<P>(" size ");
            k_boot_debug_hex::<P>(memory.size() as _);
            k_boot_debug::<P>("\r\n");
        }

        if let Ok(memory) = fdt.memory() {
            for region in memory.regions() {
                k_boot_debug::<P>("memory region: ");
                k_boot_debug_hex::<P>(region.starting_address as usize as _);
                k_boot_debug::<P>(" size ");
                k_boot_debug_hex::<P>(region.size.unwrap_or_default() as _);
                k_boot_debug::<P>("\r\n");
            }
        }

        let chosen = fdt.chosen().ok()?;
        if let Some(stdout) = chosen.stdout() {
            k_boot_debug::<P>("stdout: ");

            let node = stdout.node();
            let reg = node.reg_fix().next()?;
            let start = reg.starting_address as usize;
            let size = reg.size?;
            k_boot_debug::<P>(node.name);
            k_boot_debug::<P>(" @");
            k_boot_debug_hex::<P>(start as _);
            k_boot_debug::<P>(" size ");
            k_boot_debug_hex::<P>(size as _);
            k_boot_debug::<P>("\r\n");
        }

        Some(())
    }
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
    unsafe fn alloc(&mut self, layout: Layout) -> Option<usize> {
        let size = layout.size();
        if self.iter + size > self.end {
            return None;
        }
        let ptr = self.iter;
        self.iter += size;
        Some(ptr)
    }

    unsafe fn dealloc(&mut self, _ptr: usize, _layout: Layout) {}

    fn va_offset(&self) -> usize {
        0
    }
}

pub(crate) unsafe fn init_page_table<P: Platform>(
    access: &mut impl Access,
) -> Result<(), PagingError> {
    let mut table = P::Table::new(access)?;

    // get_device_tree().take_if(|fdt| {
    //     for region in fdt.memory_reservations() {
    //         table.map_region(
    //             MapConfig {
    //                 vaddr: region.address().add(va_offset()),
    //                 paddr: region.address() as usize,
    //                 attrs: PageAttribute::Read | PageAttribute::Write,
    //             },
    //             region.size(),
    //             true,
    //             access,
    //         )
    //     }
    // });
    // table.map_region(
    //     MapConfig {
    //         vaddr: MEMORY_START,
    //         paddr: KERNEL_START,
    //         attrs: PageAttribute::Read | PageAttribute::Write,
    //     },
    //     KERNEL_SIZE,
    //     true,
    //     access,
    // );

    let kernel_phys = Phys::<u8>::from(BOOT_INFO.reserved_start);
    let kernel_virt = kernel_phys.to_virt();
    let kernel_size = (BOOT_INFO.reserved_end - BOOT_INFO.reserved_start).align_up_4k();
    let _ = table.map_region(
        MapConfig {
            vaddr: kernel_virt.into(),
            paddr: kernel_phys.into(),
            attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
        },
        BYTES_1G,
        true,
        access,
    );

    // let vaddr = VirtAddr::from(MEMORY_START + va_offset());
    // let paddr = Phys::<u8>::from(MEMORY_START);
    // let size = MEMORY_SIZE;

    // table.map_region(
    //     MapConfig {
    //         vaddr: vaddr.as_mut_ptr(),
    //         paddr: paddr.as_usize(),
    //         attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
    //     },
    //     size,
    //     true,
    //     access,
    // )?;

    P::set_kernel_page_table(&table);
    P::set_user_page_table(None);

    Ok(())
}

pub(crate) unsafe fn iomap<P: Platform>(paddr: PhysAddr, size: usize) -> NonNull<u8> {
    let mut table = P::get_kernel_page_table();
    let paddr = paddr.align_down(0x1000);
    let vaddr = paddr.to_virt().as_mut_ptr();
    let size = size.max(0x1000);

    let mut heap = HEAP_ALLOCATOR.lock();
    let mut heap_mut = AllocatorRef::new(&mut heap);

    let _ = table.map_region_with_handle(
        MapConfig {
            vaddr,
            paddr: paddr.into(),
            attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Device,
        },
        size,
        true,
        &mut heap_mut,
        Some(&|addr| {
            P::flush_tlb(Some(addr));
        }),
    );

    NonNull::new_unchecked(vaddr)
}
