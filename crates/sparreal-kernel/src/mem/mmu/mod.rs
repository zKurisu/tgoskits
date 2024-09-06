use core::{
    alloc::Layout,
    ptr::{slice_from_raw_parts, slice_from_raw_parts_mut, NonNull},
};

use flat_device_tree::Fdt;
pub use page_table_interface::*;

static mut VA_OFFSET: usize = 0;
static mut DTB_ADDR: Option<NonNull<u8>> = None;
static mut HEAP_BEGIN_LMA: NonNull<u8> = NonNull::dangling();

pub fn va_offset() -> usize {
    unsafe { VA_OFFSET }
}

pub unsafe fn boot_init<T: PageTable>(
    va_offset: usize,
    dtb_addr: NonNull<u8>,
    mut heap_begin_lma: NonNull<u8>,
    kernel_lma: NonNull<u8>,
) {
    VA_OFFSET = va_offset;
    DTB_ADDR = protect_dtb(dtb_addr, heap_begin_lma);

    let fdt = device_tree().unwrap();
    let memory = fdt.memory().unwrap();
    let primory = memory.regions().next().unwrap();
    let memory_begin = primory.starting_address;
    let memory_size = primory.size.unwrap();

    let mut access =
        BeforeMMUPageAllocator::new(memory_begin as usize + memory_size / 2, memory_size / 2);
}

fn device_tree() -> Option<Fdt<'static>> {
    unsafe {
        let dtb_addr = DTB_ADDR?;
        Fdt::from_ptr(dtb_addr.as_ptr()).ok()
    }
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

// impl Access for BeforeMMUPageAllocator {
//     unsafe fn alloc(&mut self, layout: Layout) -> Option<NonNull<u8>> {
//         let size = layout.size();
//         if self.iter + size > self.end {
//             return None;
//         }
//         let ptr = self.iter;
//         self.iter += size;
//         NonNull::new(ptr as *mut u8)
//     }

//     unsafe fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {}

//     fn virt_to_phys<T>(&self, addr: NonNull<T>) -> usize {
//         addr.as_ptr() as usize
//     }

//     fn phys_to_virt<T>(&self, phys: usize) -> NonNull<T> {
//         unsafe { NonNull::new_unchecked(phys as *mut T) }
//     }
// }
