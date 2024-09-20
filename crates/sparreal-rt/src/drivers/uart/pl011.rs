use alloc::{boxed::Box, string::ToString};

use arm_pl011_rs::Pl011;
use driver_interface::*;
use embedded_io::*;
use future::LocalBoxFuture;
use futures::prelude::*;

pub fn register() -> Register {
    Register {
        name: "PL011".to_string(),
        compatible: ["arm,pl011"].to_vec(),
        kind: RegisterKind::Uart(Box::new(RegisterPl011 {})),
    }
}

struct RegisterPl011 {}

struct DriverPl011(Pl011);

unsafe impl Send for DriverPl011 {}
unsafe impl Sync for DriverPl011 {}

impl uart::Driver for DriverPl011 {}
impl io::Write for DriverPl011 {
    fn write(&mut self, buf: &[u8]) -> io::IOResult<usize> {
        match self.0.write(buf) {
            Ok(n) => Ok(n),
            Err(e) => Err(e.kind()),
        }
    }

    fn flush(&mut self) -> io::IOResult {
        Ok(())
    }
}

impl DriverGeneric for DriverPl011 {}

impl RegisterPl011 {
    async fn new_pl011(config: uart::Config) -> DriverResult<Box<dyn uart::Driver>> {
        let uart = Pl011::new(config.reg, Some(Self::conv_config(config))).await;

        Ok(Box::new(DriverPl011(uart)))
    }

    fn conv_config(config: uart::Config) -> arm_pl011_rs::Config {
        arm_pl011_rs::Config {
            baud_rate: config.baud_rate,
            clock_freq: config.clock_freq as _,
            data_bits: match config.data_bits {
                uart::DataBits::Bits5 => arm_pl011_rs::DataBits::Bits5,
                uart::DataBits::Bits6 => arm_pl011_rs::DataBits::Bits6,
                uart::DataBits::Bits7 => arm_pl011_rs::DataBits::Bits7,
                uart::DataBits::Bits8 => arm_pl011_rs::DataBits::Bits8,
            },
            stop_bits: match config.stop_bits {
                uart::StopBits::STOP1 => arm_pl011_rs::StopBits::STOP1,
                uart::StopBits::STOP2 => arm_pl011_rs::StopBits::STOP2,
            },
            parity: match config.parity {
                uart::Parity::None => arm_pl011_rs::Parity::None,
                uart::Parity::Even => arm_pl011_rs::Parity::Even,
                uart::Parity::Odd => arm_pl011_rs::Parity::Odd,
            },
        }
    }
}

impl uart::Register for RegisterPl011 {
    fn probe<'a>(
        &self,
        config: uart::Config,
    ) -> LocalBoxFuture<'a, DriverResult<Box<dyn uart::Driver>>> {
        Self::new_pl011(config).boxed_local()
    }
}
