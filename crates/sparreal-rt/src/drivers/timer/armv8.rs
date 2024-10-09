use core::time::Duration;

use aarch64_cpu::registers::*;
use alloc::{boxed::Box, vec};
use driver_interface::*;
use futures::{future::LocalBoxFuture, FutureExt};
use log::info;
use sparreal_kernel::irq::{register_irq, IrqConfig, IrqHandle};
use timer::Driver;

pub fn register() -> Register {
    Register::new(
        "Timer Armv8",
        vec!["arm,armv8-timer"],
        DriverKind::Timer,
        ProbeTimerArmv8 {},
    )
}

struct ProbeTimerArmv8 {}

impl Probe for ProbeTimerArmv8 {
    fn probe<'a>(&self, config: ProbeConfig) -> LocalBoxFuture<'a, DriverResult<DriverSpecific>> {
        async move {
            let irq_ns = &config.irq[1];

            register_irq(
                IrqConfig {
                    irq: irq_ns.irq,
                    trigger: irq_ns.trigger,
                    priority: 0,
                    cpu_list: vec![],
                },
                config.id,
                move |_irq| {
                    info!("armv8 timer irq!");

                    IrqHandle::Handled
                },
            );

            let timer = Box::new(DriverTimerArmv8 {});
            Ok(DriverSpecific::Timer(timer))
        }
        .boxed_local()
    }
}

struct DriverTimerArmv8;

impl DriverGeneric for DriverTimerArmv8 {}
impl timer::Driver for DriverTimerArmv8 {
    fn current_ticks(&self) -> u64 {
        CNTFRQ_EL0.get()
    }

    fn tick_hz(&self) -> u64 {
        CNTPCT_EL0.get()
    }

    fn set_interval(&mut self, ticks: u64) {
        CNTP_TVAL_EL0.set(ticks);
    }

    fn set_enable(&mut self, enable: bool) {
        CNTP_CTL_EL0.write(if enable {
            CNTP_CTL_EL0::ENABLE::SET
        } else {
            CNTP_CTL_EL0::ENABLE::CLEAR
        });
    }
}
