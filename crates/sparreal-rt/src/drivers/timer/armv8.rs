
use aarch64_cpu::registers::*;
use alloc::{boxed::Box, vec};
use driver_interface::*;
use futures::{future::LocalBoxFuture, FutureExt};
use sparreal_kernel::irq::irq_setup;
use timer::Driver;
use tock_registers::interfaces::ReadWriteable;

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
            irq_setup(irq_ns.irq, config.id, irq_ns.trigger);
            // register_irq(
            //     IrqConfig {
            //         irq: irq_ns.irq,
            //         trigger: irq_ns.trigger,
            //         priority: 0,
            //         cpu_list: vec![],
            //     },
            //     config.id,
            //     move |_irq| {
            //         let clr = CNTP_CTL_EL0.is_set(CNTP_CTL_EL0::ISTATUS);
            //         info!("armv8 timer irq! {}", clr);

            //         CNTP_CTL_EL0.write(CNTP_CTL_EL0::IMASK::SET);
            //         IrqHandle::Handled
            //     },
            // );

            let mut timer = Box::new(DriverTimerArmv8 {
                irq: irq_ns.irq as _,
            });
            // timer.set_enable(true);
            // timer.set_irq_enable(true);
            timer.set_irq_enable(false);
            // let freq = timer.tick_hz();
            // let ticks = 2 * freq;
            // timer.set_interval(ticks);
            Ok(DriverSpecific::Timer(timer))
        }
        .boxed_local()
    }
}

struct DriverTimerArmv8 {
    irq: u64,
}

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
        CNTP_CTL_EL0.modify(if enable {
            CNTP_CTL_EL0::ENABLE::SET
        } else {
            CNTP_CTL_EL0::ENABLE::CLEAR
        });
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

    fn irq_num(&self) -> u64 {
        self.irq
    }
}
