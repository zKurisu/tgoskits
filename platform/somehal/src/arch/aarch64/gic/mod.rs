use arm_gic_driver::IntId;
use rdif_intc::Intc;
use rdrive::Device;

mod v3;

fn get_gicd() -> Device<Intc> {
    rdrive::get_one().expect("no interrupt controller found")
}

pub fn init_cpu() {
    v3::with_gic(|gic| {
        let mut cpu = gic.cpu_interface();
        cpu.init_current_cpu().unwrap();
        debug!("GICC initialized");
    });
}

pub fn irq_set_enable(irq: rdrive::IrqId, enable: bool) {
    let raw: usize = irq.into();

    v3::with_gic(|gic| {
        gic.set_irq_enable(unsafe { IntId::raw(raw as _) }, enable);
    });
}
