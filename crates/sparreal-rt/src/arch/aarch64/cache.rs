use core::{arch::asm, ptr::NonNull};

use aarch64_cpu::registers::DAIF::I;
use dma_api::Impl;
use log::debug;
use sparreal_kernel::mem::{mmu::va_offset, Virt};

#[inline(always)]
fn cache_line_size() -> usize {
    unsafe {
        let mut ctr_el0: u64;
        asm!("mrs {}, ctr_el0", out(reg) ctr_el0);
        let log2_cache_line_size = ((ctr_el0 >> 16) & 0xF) as usize;
        // Calculate the cache line size
        4 << log2_cache_line_size
    }
}

struct DCacheIter {
    aligned_addr: usize,
    end: usize,
    cache_line_size: usize,
}

impl DCacheIter {
    fn new(addr: NonNull<u8>, size: usize) -> DCacheIter {
        let start = addr.as_ptr() as usize;
        let end = start + size;
        let cache_line_size = cache_line_size();

        let aligned_addr = addr.as_ptr() as usize & !(cache_line_size - 1);
        DCacheIter {
            aligned_addr,
            end,
            cache_line_size,
        }
    }
}

impl Iterator for DCacheIter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.aligned_addr < self.end {
            let addr = self.aligned_addr;
            self.aligned_addr += self.cache_line_size;
            Some(addr)
        } else {
            None
        }
    }
}

/// Invalidate data cache
pub fn dcache_invalidate_range(addr: NonNull<u8>, size: usize) {
    unsafe {
        for addr in DCacheIter::new(addr, size) {
            asm!("dc ivac, {}", in(reg) addr);
        }
        asm!("dsb sy");
    }
}

/// Flush data cache
pub fn dcache_flush_range(addr: NonNull<u8>, size: usize) {
    unsafe {
        for addr in DCacheIter::new(addr, size) {
            asm!("dc civac, {}", in(reg) addr);
        }
        asm!("dsb sy");
    }
}

struct DMAImpl;

impl Impl for DMAImpl {
    fn map(addr: NonNull<u8>, _size: usize, _direction: dma_api::Direction) -> u64 {
        let p: Virt<u8> = Virt::from(addr.as_ptr());
        p.convert_to_phys(va_offset()).as_usize() as _
    }

    fn unmap(_addr: NonNull<u8>, _size: usize) {}

    fn flush(addr: NonNull<u8>, size: usize) {
        dcache_flush_range(addr, size);
    }

    fn invalidate(addr: NonNull<u8>, size: usize) {
        dcache_invalidate_range(addr, size);
    }
}

dma_api::set_impl!(DMAImpl);
