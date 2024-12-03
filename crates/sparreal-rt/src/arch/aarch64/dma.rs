use core::{arch::asm, ptr::NonNull};

use dma_api::Impl;
use sparreal_kernel::mem::{mmu::va_offset, Virt};

/// Invalidate data cache
pub fn dcache_invalidate_range(addr: NonNull<u8>, size: usize) {
    let start = addr.as_ptr() as usize;
    let end = start + size;

    unsafe {
        asm!(
            "mrs	x3, ctr_el0",
            "ubfx	x3, x3, #16, #4",
            "mov	x2, #4",
            "lsl	x2, x2, x3", /* cache line size */
            /* x2 <- minimal cache line size in cache system */
            "sub	x3, x2, #1",
            "bic	x0, x0, x3",
            "1:	dc	ivac, x0", /* invalidate data or unified cache */
            "add	x0, x0, x2",
            "cmp	x0, x1",
            "b.lo	1b",
            "dsb	sy"
        );
    }
}

/// Flush data cache
pub fn dcache_flush_range(addr: NonNull<u8>, size: usize) {
    let start = addr.as_ptr() as usize;
    let end = start + size;
    unsafe {
        asm!(
            "mrs	x3, ctr_el0",
            "ubfx	x3, x3, #16, #4",
            "mov	x2, #4",
            "lsl	x2, x2, x3", /* cache line size */
            /* x2 <- minimal cache line size in cache system */
            "sub	x3, x2, #1",
            "bic	x0, x0, x3",
            "1:	dc	civac, x0", /* clean & invalidate data or unified cache */
            "add	x0, x0, x2",
            "cmp	x0, x1",
            "b.lo	1b",
            "dsb	sy"
        );
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
