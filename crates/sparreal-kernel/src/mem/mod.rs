pub mod mmu;

use core::ptr::NonNull;

use buddy_system_allocator::LockedHeap;
use memory_addr::PhysAddr;
use mmu::va_offset;

use crate::{
    driver::device_tree::{self, get_device_tree},
    KernelConfig,
};

#[global_allocator]
static HEAP_ALLOCATOR: LockedHeap<32> = LockedHeap::empty();

pub const BYTES_1K: usize = 1024;
pub const BYTES_1M: usize = 1024 * BYTES_1K;
pub const BYTES_1G: usize = 1024 * BYTES_1M;

pub(crate) trait VirtToPhys {
    fn to_phys(&self) -> PhysAddr;
}

impl<T> VirtToPhys for NonNull<T> {
    fn to_phys(&self) -> PhysAddr {
        (self.as_ptr() as usize - va_offset()).into()
    }
}

pub struct MemoryManager {}

impl MemoryManager {
    pub const fn new() -> Self {
        Self {}
    }

    pub unsafe fn init(&self, cfg: &KernelConfig) {
        let mut start = cfg.heap_start;
        let mut size = 2 * BYTES_1M;

        if let Some(fdt) = get_device_tree() {
            start = start.add(fdt.total_size());

            if let Some(memory) = fdt.memory().ok() {
                for region in memory.regions() {
                    let used = start.to_phys().as_usize() - region.starting_address as usize;

                    if let Some(mem_size) = region.size {
                        size = mem_size - used;
                    }
                }
            }
        }

        HEAP_ALLOCATOR.lock().init(start.as_ptr() as usize, size)
    }
}
