use aarch64_cpu::registers::*;
use alloc::{boxed::Box, vec::Vec};
use sparreal_kernel::driver_interface::{
    DriverGeneric, ProbeFnKind, interrupt_controller::IrqConfig, timer::*,
};
use sparreal_macros::module_driver;

module_driver!(
    name: "ARMv8 Timer",
    compatibles: &["arm,armv8-timer"],
    probe: ProbeFnKind::Timer(probe_timer),
);

#[derive(Clone)]
struct ArmV8Timer {
    irq: IrqConfig,
}

impl Interface for ArmV8Timer {
    fn get_current_cpu(&mut self) -> Box<dyn InterfaceCPU> {
        Box::new(self.clone())
    }
}

impl InterfaceCPU for ArmV8Timer {
    fn set_timeval(&mut self, ticks: u64) {
        CNTP_TVAL_EL0.set(ticks);
    }

    fn current_ticks(&self) -> u64 {
        CNTPCT_EL0.get()
    }

    fn tick_hz(&self) -> u64 {
        CNTFRQ_EL0.get()
    }

    fn set_irq_enable(&mut self, enable: bool) {
        CNTP_CTL_EL0.modify(if enable {
            CNTP_CTL_EL0::IMASK::CLEAR
        } else {
            CNTP_CTL_EL0::IMASK::SET
        });
    }

    fn read_irq_status(&self) -> bool {
        CNTP_CTL_EL0.is_set(CNTP_CTL_EL0::ISTATUS)
    }

    fn irq(&self) -> IrqConfig {
        self.irq.clone()
    }
}

impl DriverGeneric for ArmV8Timer {
    fn open(&mut self) -> Result<(), alloc::string::String> {
        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::ENABLE::SET);
        Ok(())
    }

    fn close(&mut self) -> Result<(), alloc::string::String> {
        CNTP_CTL_EL0.modify(CNTP_CTL_EL0::ENABLE::CLEAR);
        Ok(())
    }
}

fn probe_timer(irqs: Vec<IrqConfig>) -> Driver {
    Box::new(ArmV8Timer {
        irq: irqs[1].clone(),
    })
}
