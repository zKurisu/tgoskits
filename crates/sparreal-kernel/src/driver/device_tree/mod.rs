use core::ptr::NonNull;

use alloc::{vec, vec::Vec};
use driver_interface::ProbeConfig;

use crate::mem::mmu::iomap;

use super::{device_id_by_node_name, irq_by_id};

// #[link_section = ".data.boot"]
static mut DTB_ADDR: Option<NonNull<u8>> = None;

pub(crate) unsafe fn set_dtb_addr(addr: Option<NonNull<u8>>) {
    DTB_ADDR = addr;
}

pub fn get_device_tree2() -> Option<Fdt<'static>> {
    unsafe {
        let dtb_addr = DTB_ADDR?;
        Fdt::from_ptr(dtb_addr.as_ptr()).ok()
    }
}
pub fn get_device_tree<'a>() -> Option<fdt_parser::Fdt<'a>> {
    unsafe {
        let dtb_addr = DTB_ADDR?;
        fdt_parser::Fdt::from_ptr(dtb_addr).ok()
    }
}
pub trait FDTExtend {
    fn interrupt_list(&self) -> Vec<Vec<usize>>;
    fn probe_config(&self) -> ProbeConfig;
    fn find_clock(&self) -> Vec<u64>;
}

impl FDTExtend for fdt_parser::Node<'_> {
    fn interrupt_list(&self) -> Vec<Vec<usize>> {
        let mut ret = Vec::new();
        let info = match self.interrupts() {
            Some(i) => i,
            None => return Vec::new(),
        };
        let mut elem = Vec::new();
        for itr in info.interrupts() {
            elem.push(itr as _);

            if elem.len() == info.cell_size {
                ret.push(elem.clone());
                elem = Vec::new();
            }
        }
        ret
    }

    fn probe_config(&self) -> ProbeConfig {
        let mut config = ProbeConfig::default();
        config.id = device_id_by_node_name(self.name);

        if let Some(regs) = self.reg() {
            for reg in regs {
                let reg_base = iomap((reg.address as usize).into(), reg.size.unwrap_or(0x1000));
                config.reg.push(reg_base);
            }
        }

        let irq_origin = self.interrupt_list();

        if let Some(itr_node) = self.interrupt_parent() {
            let id = device_id_by_node_name(itr_node.node.name);
            if let Some(irq) = irq_by_id(id) {
                let g = irq.spec.read();

                for elem in irq_origin {
                    config.irq.push(g.fdt_parse_config(&elem));
                }
            }
        }

        config.clock_freq = self.find_clock();

        config
    }

    fn find_clock(&self) -> Vec<u64> {
        if let Some(clk) = self.clock_frequency() {
            vec![clk as _]
        } else {
            let mut out = Vec::new();
            for clk in self.clocks() {
                if let Some(freq) = clk.node.clock_frequency() {
                    out.push(freq as _);
                }
            }

            out
        }
    }
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
        config.id = device_id_by_node_name(self.name);

        for reg in self.reg_fix() {
            let reg_base = iomap(reg.starting_address.into(), reg.size.unwrap_or(0x1000));
            config.reg.push(reg_base);
        }

        let irq_origin = self.interrupt_list();

        if let Some(itr_node) = self.interrupt_parent() {
            let id = device_id_by_node_name(itr_node.name);
            if let Some(irq) = irq_by_id(id) {
                let g = irq.spec.read();

                for elem in irq_origin {
                    config.irq.push(g.fdt_parse_config(&elem));
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
