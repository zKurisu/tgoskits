use core::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut, NonNull};

use alloc::string::String;
use driver_interface::uart;
use flat_device_tree::Fdt;

pub mod device_tree;
pub mod manager;

pub struct Driver {
    pub name: String,
    pub kind: DriverKind,
}

pub enum DriverKind {
    Uart(uart::BoxDriver),
}

pub unsafe fn move_dtb(src: *const u8, mut dst: NonNull<u8>) -> Option<&'static [u8]> {
    let fdt = Fdt::from_ptr(src).ok()?;
    let size = fdt.total_size();
    let dest = &mut *slice_from_raw_parts_mut(dst.as_mut(), size);
    let src = &*slice_from_raw_parts(src, size);
    dest.copy_from_slice(src);
    Some(dest)
}
