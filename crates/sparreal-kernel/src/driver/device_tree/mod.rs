use core::ptr::NonNull;

use alloc::{vec, vec::Vec};
use driver_interface::ProbeConfig;
use flat_device_tree::{
    node::{CellSize, FdtNode},
    Fdt,
};
use log::debug;

use crate::mem::mmu::iomap;

use super::{driver_id_by_node_name, irq_by_id, DriverId};

// #[link_section = ".data.boot"]
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
    fn find_clock(&self) -> Vec<u64>;
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
        config.id = driver_id_by_node_name(self.name);

        for reg in self.reg_fix() {
            let reg_base = iomap(reg.starting_address.into(), reg.size.unwrap_or(0x1000));
            config.reg.push(reg_base);
        }

        let irq_origin = self.interrupt_list();

        if let Some(itr_node) = self.interrupt_parent() {
            let id = driver_id_by_node_name(itr_node.name);
            if let Some(irq) = irq_by_id(id) {
                let g = irq.spec.read();

                for elem in irq_origin {
                    config.irq.push(g.fdt_itr_to_config(&elem));
                }
            }
        }

        config.clock_freq = self.find_clock();

        config
    }

    fn find_clock(&self) -> Vec<u64> {
        if let Some(clk) = self.clock_frequency() {
            vec![clk]
        } else {
            self.clocks().filter_map(|c| c.clock_frequency()).collect()
        }
    }
}
