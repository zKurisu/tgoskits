use aarch64_cpu::registers::{CurrentEL, ICC_SRE_EL2};
use arm_gic_driver::{
    IntId,
    v3::{ICC_SRE_EL1, ack1, dir, eoi_mode, eoi1},
};
use rdif_intc::Intc;
use rdrive::Device;
use someboot::irq::IrqId;
use tock_registers::interfaces::*;

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

#[unsafe(no_mangle)]
fn __aarch64_irq_handler() {
    trace!("Handling IRQ!!!");
    let icc_enable = if CurrentEL.read(CurrentEL::EL) == 1 {
        ICC_SRE_EL1.is_set(ICC_SRE_EL1::SRE)
    } else if CurrentEL.read(CurrentEL::EL) == 2 {
        ICC_SRE_EL2.is_set(ICC_SRE_EL2::SRE)
    } else {
        panic!("Unsupported exception level for IRQ handling");
    };

    if !icc_enable {
        panic!("GIC CPU interface not enabled!");
    }

    handle_irq_v3();
}

#[allow(dead_code)]
pub(crate) fn handle_irq(hwirq: IrqId) {
    unsafe extern "Rust" {
        fn _someboot_handle_irq(hwirq: IrqId);
    }
    unsafe {
        _someboot_handle_irq(hwirq);
    }
}

fn handle_irq_v3() {
    let ack = ack1();

    handle_irq(IrqId::new(ack.to_u32() as _));

    if !ack.is_special() {
        eoi1(ack);
        if eoi_mode() {
            dir(ack);
        }
    }
}
