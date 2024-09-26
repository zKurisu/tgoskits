use alloc::{boxed::Box, vec};
use driver_interface::*;
use futures::{future::LocalBoxFuture, FutureExt};
use irq::IrqConfig;
use log::info;
use sparreal_kernel::irq::{register_irq, IrqHandle};

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

            Ok(DriverSpecific::Timer(Box::new(DriverTimerArmv8)))
        }
        .boxed_local()
    }
}

struct DriverTimerArmv8;

impl DriverGeneric for DriverTimerArmv8 {}
impl timer::Driver for DriverTimerArmv8 {}
