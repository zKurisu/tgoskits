use core::ptr::NonNull;

use alloc::vec::Vec;
use flat_device_tree::{node::FdtNode, Fdt};

#[link_section = ".data.boot"]
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

pub trait FDTExtend {
    fn interrupt_list(&self) -> Vec<Vec<usize>>;
}

impl FDTExtend for FdtNode<'_, '_> {
    fn interrupt_list(&self) -> Vec<Vec<usize>> {
        let mut ret = Vec::new();
        let (size, mut itrs) = self.interrupts();
        let mut elem = Vec::new();
        while let Some(itr) = itrs.next() {
            elem.push(itr);

            if elem.len() == size {
                ret.push(elem.clone());
                elem = Vec::new();
            }
        }
        ret
    }
}
