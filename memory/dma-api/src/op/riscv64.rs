//! RISC-V (XuanTie C906 XTheadCmo) dcache maintenance for DMA.
//!
//! The C906 core does not implement the standard Zicbom extension. Instead it
//! provides XTheadCmo instructions, which GNU as does not assemble by mnemonic,
//! so we emit the raw 32-bit encodings exactly like the vendor Linux tree does
//! (`arch/riscv/mm/cacheflush.c`) and like U-Boot's
//! `arch/riscv/lib/thead_cmo.c`:
//!
//! * `dcache.cpa a0`  = `.long 0x0295000b` — clean (write back) one line
//! * `dcache.cipa a0` = `.long 0x02b5000b` — clean + invalidate one line
//! * `sync.is`        = `.long 0x01b0000b` — complete the cache operations
//!
//! The operand is the line address in register `a0`, and on this core the CMO
//! instructions index the dcache by **physical** address (see the vendor
//! `dma_wb_range`/`dma_wbinv_range`; `op::cache_addr` resolves the physical
//! address before calling here). The Linux tree only ships clean and
//! clean+invalidate, so both `invalidate` and `flush_invalidate` map to
//! `dcache.cipa`, which is the safe choice for the "device wrote, CPU must
//! re-read" case.

use core::ptr::NonNull;

/// dcache line size on the C906.
const CACHE_LINE_BYTES: usize = 64;

#[inline]
unsafe fn dcache_cpa(addr: usize) {
    // SAFETY: caller guarantees this runs on a core implementing XTheadCmo.
    unsafe {
        core::arch::asm!(".long 0x0295000b", in("a0") addr, options(nostack));
    }
}

#[inline]
unsafe fn dcache_cipa(addr: usize) {
    // SAFETY: caller guarantees this runs on a core implementing XTheadCmo.
    unsafe {
        core::arch::asm!(".long 0x02b5000b", in("a0") addr, options(nostack));
    }
}

#[inline]
unsafe fn sync_is() {
    // SAFETY: `sync.is` has no operands and no memory side effects of its own.
    unsafe {
        core::arch::asm!(".long 0x01b0000b", options(nostack));
    }
}

#[inline]
fn cache_range(addr: NonNull<u8>, size: usize, invalidate: bool) {
    let start = addr.as_ptr() as usize & !(CACHE_LINE_BYTES - 1);
    let end = (addr.as_ptr() as usize + size + CACHE_LINE_BYTES - 1) & !(CACHE_LINE_BYTES - 1);
    // Order prior memory accesses before the cache operations so a `dcache.cpa`
    // (clean) sees every pending store.
    unsafe { core::arch::asm!("fence rw, rw") };
    for line in (start..end).step_by(CACHE_LINE_BYTES) {
        unsafe {
            if invalidate {
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
