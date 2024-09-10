use core::ptr::NonNull;

use flat_device_tree::Fdt;

static mut DTB_ADDR: Option<NonNull<u8>> = None;

pub(crate) unsafe fn set_dtb_addr(addr: Option<NonNull<u8>>) {
    DTB_ADDR = addr;
}

pub fn get_device_tree() -> Option<Fdt<'static>> {
    unsafe {
        let dtb_addr = DTB_ADDR?;
        Fdt::from_ptr(dtb_addr.as_ptr()).ok()
    }
}
