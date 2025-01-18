use core::ptr::NonNull;

use alloc::{
    collections::btree_map::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
pub use driver_interface::interrupt_controller::Driver;
use driver_interface::{DriverRegister, ProbeFnKind, RegAddress, interrupt_controller::IrqConfig};
use fdt_parser::Fdt;

use super::{super::device::Descriptor, Device, DriverId};

pub fn init_by_fdt(
    registers: &[DriverRegister],
    fdt_addr: NonNull<u8>,
) -> Result<Vec<Device<Driver>>, String> {
    let fdt = Fdt::from_ptr(fdt_addr).map_err(|e| format!("{e:?}"))?;
    let mut out = Vec::with_capacity(registers.len());
    for r in registers {
        if let ProbeFnKind::InterruptController(probe) = r.probe {
            let compa = r
                .compatibles
                .iter()
                .filter_map(|e| if e.is_empty() { None } else { Some(*e) })
                .collect::<Vec<_>>();
            for node in fdt.find_compatible(&compa) {
                let reg = node
                    .reg()
                    .ok_or(format!("[{}] has no reg", node.name))?
                    .map(|reg| RegAddress {
                        addr: reg.address as _,
                        size: reg.size,
                    })
                    .collect();
                let mut irq = probe(reg);
                irq.open()?;
                let dev = Device::new(
                    Descriptor {
                        driver_id: node.phandle().unwrap().as_usize().into(),
                        name: r.name.to_string(),
                        ..Default::default()
                    },
                    irq,
                );

                out.push(dev);
            }
        }
    }
    Ok(out)
}

pub struct Container(BTreeMap<DriverId, Device<Driver>>);

impl Container {
    pub const fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub(crate) fn set(&mut self, dev: Device<Driver>) {
        self.0.insert(dev.descriptor.driver_id, dev);
    }

    pub(crate) fn set_list(&mut self, dev_list: Vec<Device<Driver>>) {
        for dev in dev_list {
            self.set(dev);
        }
    }

    pub fn list(&self) -> Vec<&Device<Driver>> {
        self.0.values().collect()
    }
}

#[derive(Default, Debug, Clone)]
pub struct IrqInfo {
    pub irq_parent: DriverId,
    pub cfgs: Vec<IrqConfig>,
}
