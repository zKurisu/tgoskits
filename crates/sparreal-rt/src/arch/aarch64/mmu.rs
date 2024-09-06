use core::{alloc::Layout, arch::asm, cell::UnsafeCell, ptr::NonNull, sync::atomic::AtomicU64};

use aarch64::{DescriptorAttr, PTE};
use aarch64_cpu::{asm::barrier, registers::*};
use page_table::*;
use sparreal_kernel::mem::mmu;
use tock_registers::interfaces::ReadWriteable;

use crate::KernelConfig;

const BYTES_1G: usize = 1024 * 1024 * 1024;

const MAIR_VALUE: u64 = {
    // Device-nGnRE memory
    let attr0 = MAIR_EL1::Attr0_Device::nonGathering_nonReordering_EarlyWriteAck.value;
    // Normal memory
    let attr1 = MAIR_EL1::Attr1_Normal_Inner::WriteBack_NonTransient_ReadWriteAlloc.value
        | MAIR_EL1::Attr1_Normal_Outer::WriteBack_NonTransient_ReadWriteAlloc.value;
    let attr2 = MAIR_EL1::Attr2_Normal_Inner::NonCacheable.value
        | MAIR_EL1::Attr2_Normal_Outer::NonCacheable.value;
    attr0 | attr1 | attr2 // 0x44_ff_04
};

pub type PageTableRef = page_table::PageTableRef<PTE, 4>;

#[allow(unused)]
#[repr(C)]
enum AttrIndex {
    Device = 0,
    Normal = 1,
    NonCacheable = 2,
}
extern "C" {
    fn _skernel();
    fn _stack_top();
}

// struct BootTable {
//     table: PageTableRef,
// }
// impl mmu::PageTable for BootTable {
//     unsafe fn new(access: &mut impl mmu::Access) -> Self {
//         todo!()
//     }

//     unsafe fn map(
//         &mut self,
//         vaddr: VirtAddr,
//         paddr: PhysAddr,
//         page_size: usize,
//         attrs: impl Iterator<Item = mmu::PageAttribute>,
//         access: &mut impl mmu::Access,
//     ) -> mmu::PagingResult {
//         todo!()
//     }
    
//     type PTE = PTE;
// }

pub unsafe fn init_boot_table(va_offset: usize, dtb_addr: NonNull<u8>) -> u64 {
    let heap_lma = NonNull::new_unchecked(_stack_top as *mut u8);
    let kernel_lma = NonNull::new_unchecked(_skernel as *mut u8);

    // mmu::boot_init::<BootTable>(va_offset, dtb_addr, heap_lma, kernel_lma);

    let mut access =
        BeforeMMUPageAllocator::new(heap_lma.as_ptr() as usize + 4096 * 32, 1024 * 4096);

    let mut table = PageTableRef::try_new(&mut access).unwrap();

    let virt_p = VirtAddr::from(kernel_lma.as_ptr() as usize).align_down(BYTES_1G);
    let phys = PhysAddr::from(virt_p.as_usize());
    let virt = virt_p + va_offset;

    let _ = table.map_region(
        virt_p,
        phys,
        BYTES_1G,
        DescriptorAttr::new(AttrIndex::Normal as u64) | DescriptorAttr::UXN,
        true,
        &mut access,
    );
    let _ = table.map_region(
        virt,
        phys,
        BYTES_1G,
        DescriptorAttr::new(AttrIndex::Normal as u64) | DescriptorAttr::UXN,
        true,
        &mut access,
    );

    MAIR_EL1.set(MAIR_VALUE);

    table.paddr().as_usize() as _
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
    unsafe fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        let size = layout.size();
        if self.iter + size > self.end {
            return None;
        }
        let ptr = self.iter;
        self.iter += size;
        NonNull::new(ptr as *mut u8)
    }

    unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {}

    fn virt_to_phys<T>(&self, addr: NonNull<T>) -> usize {
        addr.as_ptr() as usize
    }

    fn phys_to_virt<T>(&self, phys: usize) -> NonNull<T> {
        unsafe { NonNull::new_unchecked(phys as *mut T) }
    }
}
