use core::{arch::asm, ptr::NonNull};

use dma_api::Impl;
use sparreal_kernel::mem::{mmu::va_offset, Virt};

pub fn dcache_line_size() -> usize {
    unsafe {
        let result;
        asm!(
            "mrs    x8, CTR_EL0",
            "ubfm x8, x8, #16, #19",	// cache line size encoding
            "mov		{0}, #4",		// bytes per word
            "lsl		{0}, {0}, x8",	// actual cache line size""",
            out(reg) result);

        result
    }
}

fn dcache_invalidate_range(addr: NonNull<u8>, size: usize) {
    let addr = addr.as_ptr() as usize;
    unsafe {
        let line_size = dcache_line_size();
        let start = addr & !(line_size - 1);
        let end = (addr + size + line_size - 1) & !(line_size - 1);

        for addr in (start..end).step_by(line_size) {
            asm!("dc ivac, {0}", in(reg) addr);
        }

        asm!("dsb sy");
    }
}

fn dcache_clean_range(addr: NonNull<u8>, size: usize) {
    let addr = addr.as_ptr() as usize;
    unsafe {
        let line_size = dcache_line_size();
        let start = addr & !(line_size - 1);
        let end = (addr + size + line_size - 1) & !(line_size - 1);

        for addr in (start..end).step_by(line_size) {
            asm!("dc cvac, {0}", in(reg) addr);
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
        dcache_clean_range(addr, size);
    }

    fn invalidate(addr: NonNull<u8>, size: usize) {
        dcache_invalidate_range(addr, size);
    }
}

dma_api::set_impl!(DMAImpl);
