use core::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut, NonNull};

use alloc::{string::String, sync::Arc};
use driver_interface::uart;
use flat_device_tree::Fdt;

use crate::sync::{RwLock, RwLockWriteGuard};

pub mod device_tree;
pub mod manager;

#[derive(Clone)]
pub struct DriverLocked {
    pub inner: Arc<RwLock<Driver>>,
}

impl DriverLocked {
    pub fn new(name: String, kind: DriverKind) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Driver { name, kind })),
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, Driver> {
        self.inner.write()
    }
}

pub struct Driver {
    pub name: String,
    pub kind: DriverKind,
}

pub enum DriverKind {
    Uart(uart::BoxDriver),
    Spi,
    I2c,
}

pub unsafe fn move_dtb(src: *const u8, mut dst: NonNull<u8>) -> Option<&'static [u8]> {
    let fdt = Fdt::from_ptr(src).ok()?;
    let size = fdt.total_size();
    let dest = &mut *slice_from_raw_parts_mut(dst.as_mut(), size);
    let src = &*slice_from_raw_parts(src, size);
    dest.copy_from_slice(src);
    Some(dest)
}
