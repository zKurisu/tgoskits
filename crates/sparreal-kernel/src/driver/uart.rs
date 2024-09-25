pub use driver_interface::uart::*;
use driver_interface::RegisterKind;
use flat_device_tree::node::{CellSize, FdtNode};
use log::{debug, info};

use crate::{driver::device_tree::FDTExtend as _, irq::fdt_get_config, mem::mmu::iomap};

// impl Manager {
//     pub(super) async fn node_probe_uart(&mut self, node: FdtNode<'_, '_>) -> Option<BoxDriver> {
//         let caps = node.compatible()?;
//         for one in caps.all() {
//             for register in self.registers.values() {
//                 if register.compatible_matched(one) {
//                     if let RegisterKind::Uart(ref register) = register.kind {
//                         info!("Probe {} - uart: {}", node.name, one);
//                         let reg = node.reg_fix().next()?;
//                         let start = (reg.starting_address as usize).into();
//                         let size = reg.size?;

//                         info!("    @{} size: {:#X}", start, size);

//                         let reg_base = iomap(start, size);

//                         let clock_freq = if let Some(clk) = get_uart_clk(&node) {
//                             clk
//                         } else {
//                             0
//                         };

//                         let itrs = node.interrupt_list();

//                         debug!("Irq: {:?}", itrs);

//                         let irq_config = fdt_get_config(&itrs[0]).unwrap();

//                         info!("    clk: {}", clock_freq);

//                         let config = Config {
//                             reg: reg_base,
//                             baud_rate: 115200,
//                             clock_freq,
//                             data_bits: DataBits::Bits8,
//                             stop_bits: StopBits::STOP1,
//                             parity: Parity::None,
//                             interrupt: irq_config,
//                         };
//                         let uart = register.probe(config).await.ok()?;

//                         info!("    probe success!");

//                         return Some(uart);
//                     }
//                 }
//             }
//         }
//         None
//     }
// }
// fn get_uart_clk(uart_node: &FdtNode<'_, '_>) -> Option<u64> {
//     let fdt = get_device_tree()?;
//     let clk = uart_node.property("clocks")?;
//     for phandle in clk.iter_cell_size(CellSize::One) {
//         if let Some(node) = fdt.find_phandle(phandle as _) {
//             return node.property("clock-frequency")?.as_usize().map(|c| c as _);
//         }
//     }
//     None
// }
