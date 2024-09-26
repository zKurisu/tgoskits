use core::ptr::NonNull;

use alloc::vec::Vec;
use driver_interface::ProbeConfig;
use flat_device_tree::{node::FdtNode, Fdt};
use log::debug;

use crate::mem::mmu::iomap;

use super::{irq_by_id, DriverId};

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
    fn probe_config(&self) -> ProbeConfig;
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

    fn probe_config(&self) -> ProbeConfig {
        let mut config = ProbeConfig::default();

        for reg in self.reg_fix() {
            let reg_base = iomap(reg.starting_address.into(), reg.size.unwrap_or(0x1000));
            config.reg.push(reg_base);
        }

        let irq_origin = self.interrupt_list();

        if let Some(itr_node) = self.interrupt_parent() {
            let id: DriverId = itr_node.name.into();
            if let Some(irq) = irq_by_id(id) {
                let g = irq.read();

                for elem in irq_origin {
                    config.irq.push(g.fdt_itr_to_config(&elem));
                }
            }
        }

        config
    }
}
