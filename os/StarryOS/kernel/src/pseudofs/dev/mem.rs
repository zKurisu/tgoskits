use core::any::Any;

use ax_hal::mem::{PhysAddr, phys_to_virt};
use ax_memory_addr::PhysAddrRange;
use axfs_ng_vfs::{NodeFlags, VfsResult};

use crate::pseudofs::{DeviceMmap, DeviceOps};

pub(crate) struct MemDev;

impl DeviceOps for MemDev {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> VfsResult<usize> {
        let phys_addr = offset as usize;
        let vaddr = phys_to_virt(PhysAddr::from_usize(phys_addr)).as_usize();
        if buf.is_empty() {
            return Ok(0);
        }
        unsafe {
            core::ptr::copy_nonoverlapping(vaddr as *const u8, buf.as_mut_ptr(), buf.len());
        }
        Ok(buf.len())
    }

    fn write_at(&self, buf: &[u8], offset: u64) -> VfsResult<usize> {
        let phys_addr = offset as usize;
        let vaddr = phys_to_virt(PhysAddr::from_usize(phys_addr)).as_usize();
        if buf.is_empty() {
            return Ok(0);
        }
        unsafe {
            core::ptr::copy_nonoverlapping(buf.as_ptr(), vaddr as *mut u8, buf.len());
        }
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
