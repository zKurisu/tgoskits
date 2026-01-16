use rdif_intc::Intc;
use rdrive::Device;

mod v3;

fn get_gicd() -> Device<Intc> {
    rdrive::get_one().expect("no interrupt controller found")
}

pub fn init() {
    let intc = get_gicd();
    debug!("Initializing GICD...");
    let mut gic = intc.lock().unwrap();
    gic.open().unwrap();
    debug!("GICD initialized");
}

pub fn init_cpu() {
    v3::with_gic(|gic| {
        let mut cpu = gic.cpu_interface();
        cpu.init_current_cpu().unwrap();
        debug!("GICC initialized");
    });
}

pub fn irq_set_enable(irq: rdrive::IrqId, enable: bool) {
    v3::with_gic(|gic| {
        gic.set_irq_enable(irq.into(), enable);
    });
}
