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

use core::any::Any;

use ax_hal::mem::{PhysAddr, phys_to_virt};
use ax_memory_addr::PhysAddrRange;
use axfs_ng_vfs::{NodeFlags, VfsResult};

use crate::pseudofs::{DeviceMmap, DeviceOps};

pub(crate) struct MemDev;

impl MemDev {
    /// Translates one physical offset into a dereferenceable kernel address.
    fn phys_alias(offset: u64) -> usize {
        phys_to_virt(PhysAddr::from_usize(offset as usize)).as_usize()
    }
}

impl DeviceOps for MemDev {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> VfsResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let src = Self::phys_alias(offset) as *const u8;
        // SAFETY: the caller owns `buf`; the kernel linear mapping makes the
        // whole physical address space dereferenceable, and `/dev/mem` is the
        // documented unrestricted window over it.
        unsafe { core::ptr::copy_nonoverlapping(src, buf.as_mut_ptr(), buf.len()) };
        Ok(buf.len())
    }

    fn write_at(&self, buf: &[u8], offset: u64) -> VfsResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let dst = Self::phys_alias(offset) as *mut u8;
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
