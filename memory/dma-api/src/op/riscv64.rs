//! RISC-V (XuanTie C906 XTheadCmo) dcache maintenance for DMA.
//!
//! The C906 core does not implement the standard Zicbom extension. Instead it
//! provides XTheadCmo instructions, which GNU as (the version used here) does
//! not assemble by mnemonic, so we emit the raw 32-bit encodings exactly like
//! the vendor Linux tree does (`arch/riscv/mm/cacheflush.c`):
//!
//! * `dcache.cpa a0`  = `.long 0x0295000b` — clean (write back) one line
//! * `dcache.cipa a0` = `.long 0x02b5000b` — clean + invalidate one line
//!
//! The operand is the line address in register `a0`. The Linux tree only ships
//! clean (`dma_wb_range`) and clean+invalidate (`dma_wbinv_range`), so both
//! `invalidate` and `flush_invalidate` map to `dcache.cipa` (clean+invalidate),
//! which is the safe choice for the "device wrote, CPU must re-read" case.

use core::ptr::NonNull;

/// dcache line size on the C906.
const CACHE_LINE_BYTES: usize = 64;

#[inline]
unsafe fn dcache_cpa(addr: usize) {
    unsafe {
        core::arch::asm!(".long 0x0295000b", in("a0") addr);
    }
}

#[inline]
unsafe fn dcache_cipa(addr: usize) {
    unsafe {
        core::arch::asm!(".long 0x02b5000b", in("a0") addr);
    }
}

#[inline]
unsafe fn sync_is() {
    unsafe {
        core::arch::asm!(".long 0x01b0000b");
    }
}

#[inline]
fn cache_range(addr: NonNull<u8>, size: usize, cipa: bool) {
    let start = addr.as_ptr() as usize & !(CACHE_LINE_BYTES - 1);
    let end = (addr.as_ptr() as usize + size + CACHE_LINE_BYTES - 1) & !(CACHE_LINE_BYTES - 1);
    // Order prior memory accesses before the cache operations so a
    // `dcache.cpa` (clean) sees every pending store.
    unsafe { core::arch::asm!("fence rw, rw") };
    for line in (start..end).step_by(CACHE_LINE_BYTES) {
        unsafe {
            if cipa {
                dcache_cipa(line);
            } else {
                dcache_cpa(line);
            }
        }
    }
    // Make the cache operations visible to later accesses.
    unsafe { core::arch::asm!("fence rw, rw") };
    unsafe { sync_is() };
}

pub fn flush(addr: NonNull<u8>, size: usize) {
    cache_range(addr, size, false);
}

pub fn invalidate(addr: NonNull<u8>, size: usize) {
    cache_range(addr, size, true);
}

pub fn flush_invalidate(addr: NonNull<u8>, size: usize) {
    cache_range(addr, size, true);
}
