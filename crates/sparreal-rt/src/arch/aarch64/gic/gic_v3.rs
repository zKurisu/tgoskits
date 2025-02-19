use core::error::Error;

use alloc::{boxed::Box, format};
use arm_gic_driver::{fdt_parse_irq_config, v3::Gic};
use fdt_parser::Node;
use sparreal_kernel::{
    driver::{
        module_driver,
        register::{intc::FdtProbeInfo, *},
    },
    mem::iomap,
};

module_driver!(
    name: "GICv3",
    probe_kinds: &[
        ProbeKind::Fdt {
            compatibles: &["arm,gic-v3"],
            on_probe: OnProbeKindFdt::Intc(probe_gic)
        }
    ]
);

fn probe_gic(node: Node<'_>) -> Result<FdtProbeInfo, Box<dyn Error>> {
    let mut reg = node.reg().ok_or(format!("[{}] has no reg", node.name))?;

    let gicd_reg = reg.next().unwrap();
    let gicr_reg = reg.next().unwrap();
    let gicd = iomap(
        (gicd_reg.address as usize).into(),
        gicd_reg.size.unwrap_or(0x1000),
    );
    let gicr = iomap(
        (gicr_reg.address as usize).into(),
        gicr_reg.size.unwrap_or(0x1000),
    );

    Ok(FdtProbeInfo {
        hardware: Box::new(Gic::new(gicd, gicr, Default::default())),
        fdt_parse_config_fn: fdt_parse_irq_config,
    })
}
