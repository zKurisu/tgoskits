use alloc::{boxed::Box, format, vec::Vec};
use core::error::Error;

use arm_gic_driver::v2::Gic;
use sparreal_kernel::{
    driver::{module_driver, probe::HardwareKind, register::*},
    mem::iomap,
};

module_driver!(
    name: "GICv2",
    kind: DriverKind::Intc,
    probe_kinds: &[
        ProbeKind::Fdt {
            compatibles: &["arm,cortex-a15-gic", "arm,gic-400"],
            on_probe: probe_gic
        },
    ] ,
);

fn probe_gic(info: FdtInfo<'_>) -> Result<Vec<HardwareKind>, Box<dyn Error>> {
    let mut reg = info
        .node
        .reg()
        .ok_or(format!("[{}] has no reg", info.node.name))?;

    let gicd_reg = reg.next().unwrap();
    let gicc_reg = reg.next().unwrap();
    let gicd = iomap(
        (gicd_reg.address as usize).into(),
        gicd_reg.size.unwrap_or(0x1000),
    );
    let gicc = iomap(
        (gicc_reg.address as usize).into(),
        gicc_reg.size.unwrap_or(0x1000),
    );
    Ok(alloc::vec![HardwareKind::Intc(Box::new(Gic::new(
        gicd, gicc
    )))])
}
