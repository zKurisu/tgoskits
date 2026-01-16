use alloc::format;
use arm_gic_driver::v3::Gic;
use rdrive::{PlatformDevice, module_driver, probe::OnProbeError, register::FdtInfo};

use crate::common::ioremap;

pub fn with_gic(f: impl FnOnce(&mut Gic)) {
    let mut gic = super::get_gicd().lock().unwrap();
    if let Some(gic) = gic.typed_mut::<Gic>() {
        f(gic);
    }
}

module_driver!(
    name: "GICv3",
    level: ProbeLevel::PreKernel,
    priority: ProbePriority::INTC,
    probe_kinds: &[
        ProbeKind::Fdt {
            compatibles: &["arm,gic-v3"],
            on_probe: probe_gic
        }
    ],
);

fn probe_gic(info: FdtInfo<'_>, dev: PlatformDevice) -> Result<(), OnProbeError> {
    let mut reg = info.node.reg().ok_or(OnProbeError::other(format!(
        "[{}] has no reg",
        info.node.name()
    )))?;

    let gicd_reg = reg.next().unwrap();
    let gicr_reg = reg.next().unwrap();

    let gicd = ioremap(gicd_reg.address as _, gicd_reg.size.unwrap_or(0x1000))?;
    let gicr = ioremap(gicr_reg.address as _, gicr_reg.size.unwrap_or(0x1000))?;

    let mut gic = unsafe { Gic::new(gicd.into(), gicr.into()) };
    gic.init();
    let mut cpu = gic.cpu_interface();
    cpu.init_current_cpu().unwrap();
    dev.register(rdif_intc::Intc::new(gic));

    Ok(())
}
