//! `/dev/mem` — raw physical-memory window (Linux char major 1, minor 1).
//!
//! SG2002 board bring-up on this platform is partly register work that has no
//! kernel driver yet: pin muxing (`0x0300_1064` and friends), UART pad
//! selection, reset-controller pokes. The vendor scripts and our own tools do
//! those writes through `/dev/mem`, so the node has to exist for the card's
//! `/root/init.sh` and application tooling to drive the board.
//!
//! Semantics follow Linux: `offset` is a *physical* address, reads and writes
//! are uncached straight through the kernel's linear mapping, and `mmap`
//! requests are turned into physical mappings by the caller. Access is
//! intentionally unrestricted (no `CONFIG_STRICT_DEVMEM` analogue); this is a
//! bring-up platform and the node is only reachable by root.

use alloc::vec::Vec;
use core::any::Any;

use ax_hal::mem::{PAGE_SIZE_4K, PhysAddr};
use ax_memory_addr::PhysAddrRange;
use axfs_ng_vfs::{NodeFlags, VfsError, VfsResult};

use crate::{
    pseudofs::{DeviceMmap, DeviceOps},
    sync::Mutex,
};

pub(crate) struct MemDev;

impl MemDev {
    /// Translates one physical range into a dereferenceable kernel address.
    ///
    /// Every requested range is mapped on demand with `ax_mm::iomap`.
    ///
    /// This platform sets `PHYS_VIRT_OFFSET == 0` and installs no static linear
    /// window that covers device MMIO, so a raw `phys_to_virt` dereference of a
    /// register address faults (and the identity map cannot be assumed to cover
    /// firmware-reserved DRAM either). `iomap` validates the range and returns a
    /// proper kernel mapping; the translation is memoized because each call
    /// carves out a fresh kernel VA range.
    ///
    /// The alias is mapped device/uncached, which is what `/dev/mem` users
    /// expect for registers. Cacheable aliasing of RAM through this node is
    /// intentionally not offered — use `mmap()` (which goes through the VMA
    /// device-mapping path) when a cached mapping is wanted.
    fn alias(paddr: usize, size: usize) -> VfsResult<usize> {
        let page = paddr & !(PAGE_SIZE_4K - 1);
        let offset = paddr - page;
        let len = (offset + size).div_ceil(PAGE_SIZE_4K) * PAGE_SIZE_4K;

        static CACHE: Mutex<Vec<(usize, usize, usize)>> = Mutex::new(Vec::new());
        let mut cache = CACHE.lock();
        if let Some((_, _, vaddr)) = cache
            .iter()
            .find(|(cached_page, cached_len, _)| *cached_page == page && *cached_len >= len)
        {
            return Ok(vaddr + offset);
        }
        let vaddr = ax_mm::iomap(PhysAddr::from_usize(page), len)
            .map_err(|_| VfsError::Io)?
            .as_usize();
        cache.push((page, len, vaddr));
        Ok(vaddr + offset)
    }
}

impl DeviceOps for MemDev {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> VfsResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let src = Self::alias(offset as usize, buf.len())? as *const u8;
        // SAFETY: the caller owns `buf`; the kernel linear mapping makes the
        // RAM window dereferenceable and MMIO was mapped just above. `/dev/mem`
        // is the documented unrestricted window over the physical space.
        unsafe { core::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), buf.len()) };
        Ok(buf.len())
    }

    fn write_at(&self, buf: &[u8], offset: u64) -> VfsResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let dst = Self::alias(offset as usize, buf.len())? as *mut u8;
        // SAFETY: as above; writes go straight to the physical window the
        // caller asked for.
        unsafe { core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, buf.len()) };
        Ok(buf.len())
    }

    fn mmap(&self, offset: u64, length: u64) -> DeviceMmap {
        DeviceMmap::Physical(
            PhysAddrRange::from_start_size(
                ax_memory_addr::PhysAddr::from(offset as usize),
                length as usize,
            ),
            None,
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn flags(&self) -> NodeFlags {
        NodeFlags::NON_CACHEABLE | NodeFlags::STREAM
    }
}
