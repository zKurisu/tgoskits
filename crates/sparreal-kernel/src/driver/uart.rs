use alloc::{collections::BTreeMap, string::String};
pub use driver_interface::uart::*;
use driver_interface::{Register, RegisterKind};
use flat_device_tree::node::FdtNode;
use log::info;

use crate::mem::mmu::iomap;

// pub(crate) trait FdtUartConfig {
//     fn get_uart_config(&self) -> Config;
// }

// impl FdtUartConfig for FdtNode<'_, '_> {
//     fn get_uart_config(&self, registers: &BTreeMap<String, Register>) -> Config {
//         let caps = self.compatible()?;
//         for one in caps.all() {
//             for register in registers.values() {
//                 if register.compatible_matched(one) {
//                     if let RegisterKind::Uart(ref register) = register.kind {
//                         info!("Probe {} - uart: {}", self.name, one);
//                         let reg = self.reg_fix().next()?;
//                         let start = (reg.starting_address as usize).into();
//                         let size = reg.size?;

//                         info!("    @{} size: {:#X}", start, size);

//                         let reg_base = iomap(start, size);

//                         let clock_freq = if let Some(clk) = get_uart_clk(&node) {
//                             clk
//                         } else {
//                             0
//                         };

//                         info!("    clk: {}", clock_freq);

//                         let config = Config {
//                             reg: reg_base,
//                             baud_rate: 115200,
//                             clock_freq,
//                             data_bits: DataBits::Bits8,
//                             stop_bits: StopBits::STOP1,
//                             parity: Parity::None,
//                         };

//                         return Some(config);
//                     }
//                 }
//             }
//         }
//     }
// }
