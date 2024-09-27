use core::time::Duration;

use aarch64_cpu::registers::*;
use alloc::{boxed::Box, vec};
use driver_interface::*;
use futures::{future::LocalBoxFuture, FutureExt};
use irq::IrqConfig;
use log::info;
use sparreal_kernel::irq::{register_irq, IrqHandle};
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
            CNTP_CTL_EL0.write(CNTP_CTL_EL0::ENABLE::SET);
            CNTP_TVAL_EL0.set(0);

            register_irq(
                IrqConfig {
                    irq_id: irq_ns.irq_id,
                    trigger: irq_ns.trigger,
                    priority: 0,
                    cpu_list: vec![0],
                },
                config.id,
                move |irq| {
                    info!("armv8 timer irq!");
                    IrqHandle::Handled
                },
            );

            register_irq(
                IrqConfig {
                    irq_id: config.irq[0].irq_id,
                    trigger: config.irq[0].trigger,
                    priority: 0,
                    cpu_list: vec![0],
                },
                config.id,
                move |irq| {
                    info!("armv8 timer irq!");
                    IrqHandle::Handled
                },
            );

            register_irq(
                IrqConfig {
                    irq_id: config.irq[2].irq_id,
                    trigger: config.irq[2].trigger,
                    priority: 0,
                    cpu_list: vec![0],
                },
                config.id,
                move |irq| {
                    info!("armv8 timer irq!");
                    IrqHandle::Handled
                },
            );
            register_irq(
                IrqConfig {
                    irq_id: config.irq[3].irq_id,
                    trigger: config.irq[3].trigger,
                    priority: 0,
                    cpu_list: vec![0],
                },
                config.id,
                move |irq| {
                    info!("armv8 timer irq!");
                    IrqHandle::Handled
                },
            );

            let timer = Box::new(DriverTimerArmv8 {});

            timer.set_one_shot(Duration::from_millis(20));
            Ok(DriverSpecific::Timer(timer))
        }
        .boxed_local()
    }
}

struct DriverTimerArmv8;

impl DriverGeneric for DriverTimerArmv8 {}
impl timer::Driver for DriverTimerArmv8 {
    fn set_one_shot(&self, delay: Duration) {
        let freq = CNTFRQ_EL0.get();
        let cnptct = CNTPCT_EL0.get();
        let ticks = delay.as_nanos() * freq as u128 / 1_000_000_000;

        let cnptct_deadline = cnptct + ticks as u64;
        if cnptct < cnptct_deadline {
            let interval = cnptct_deadline - cnptct;
            debug_assert!(interval <= u32::MAX as u64);
            CNTP_TVAL_EL0.set(interval);
        } else {
            CNTP_TVAL_EL0.set(0);
        }
    }
}
