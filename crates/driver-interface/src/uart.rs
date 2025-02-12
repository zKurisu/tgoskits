use fdt_parser::Node;

use crate::{
    DriverGeneric, IrqHandleResult,
    io::{Read, Write},
};

pub type Hardware = alloc::boxed::Box<dyn Interface>;
/// The function to probe the hardware.
/// The first parameter is the ptr of fdt.
pub type OnProbeFdt = fn(node: Node<'_>) -> Hardware;

pub trait Interface: DriverGeneric + Write + Read + Sync {
    fn handle_irq(&mut self, irq: usize) -> IrqHandleResult;
    fn irq_enable(&mut self);
    fn irq_disable(&mut self);
    fn set_baudrate(&mut self, baudrate: u64);
    fn set_databits(&mut self, databits: DataBits);
    fn set_stopbits(&mut self, stopbits: StopBits);
    fn set_parity(&mut self, parity: Option<Parity>);
}

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
