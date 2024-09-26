use alloc::{boxed::Box, vec};
use driver_interface::*;
use futures::{future::LocalBoxFuture, FutureExt};

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
        async move { Ok(DriverSpecific::Timer(Box::new(DriverTimerArmv8))) }.boxed_local()
    }
}

struct DriverTimerArmv8;

impl DriverGeneric for DriverTimerArmv8 {}
impl timer::Driver for DriverTimerArmv8 {}
