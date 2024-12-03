use core::{arch::global_asm, ptr::NonNull};

use dma_api::Impl;
use sparreal_kernel::mem::{mmu::va_offset, Virt};

global_asm!(include_str!("cache.S"));

/// Invalidate data cache
pub fn dcache_invalidate_range(addr: NonNull<u8>, size: usize) {
    extern "C" {
        fn __asm_invalidate_dcache_range(start: usize, end: usize);
    }

    unsafe {
        __asm_invalidate_dcache_range(addr.as_ptr() as _, addr.add(size).as_ptr() as _);
    }
}

/// Flush data cache
pub fn dcache_flush_range(addr: NonNull<u8>, size: usize) {
    extern "C" {
        fn __asm_flush_dcache_range(start: usize, end: usize);
    }

    unsafe {
        __asm_flush_dcache_range(addr.as_ptr() as _, addr.add(size).as_ptr() as _);
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
