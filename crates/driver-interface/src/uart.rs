use core::ptr::NonNull;

use alloc::boxed::Box;

use crate::{io, irq::IrqConfig, DriverResult};

pub trait Driver: super::DriverGeneric + io::Write {}

pub type BoxDriver = Box<dyn Driver>;

/// Word length.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DataBits {
    Bits5,
    Bits6,
    Bits7,
    Bits8,
}

/// Parity bit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Parity {
    None,
    Even,
    Odd,
}

/// Stop bits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StopBits {
    #[doc = "1 stop bit"]
    STOP1,
    #[doc = "2 stop bits"]
    STOP2,
}

pub struct Config {
    pub reg: NonNull<u8>,
    pub baud_rate: u32,
    pub clock_freq: u64,
    pub data_bits: DataBits,
    pub stop_bits: StopBits,
    pub parity: Parity,
    pub interrupt: IrqConfig,
}
