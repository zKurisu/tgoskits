use alloc::{
    boxed::Box,
    collections::{btree_map::BTreeMap, btree_set::BTreeSet},
    string::String,
    sync::Arc,
    vec::Vec,
};
use driver_interface::*;
use flat_device_tree::node::{CellSize, FdtNode};

use crate::{module::ModuleBase, sync::RwLock, Platform};

use super::device_tree::get_device_tree;

pub struct DriverManager<P: Platform> {
    inner: Arc<RwLock<Manager<P>>>,
}

impl<P: Platform> Clone for DriverManager<P> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<P: Platform> DriverManager<P> {
    pub fn new(module: ModuleBase<P>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Manager::new(module))),
        }
    }

    pub async fn init(&self) {
        self.inner.write().init().await;
    }
    pub async fn init_stdout(&self) {
        self.inner.write().init_stdout().await;
    }
    pub fn register(&self, register: Register) {
        self.inner
            .write()
            .registers
            .insert(register.name.clone(), register);
    }

    pub fn register_all(&self, list: impl IntoIterator<Item = Register>) {
        let mut g = self.inner.write();
        for reg in list {
            g.registers.insert(reg.name.clone(), reg);
        }
    }
}

struct Manager<P: Platform> {
    module: ModuleBase<P>,
    registers: BTreeMap<String, Register>,
    registed: BTreeSet<String>,
    uart: Vec<Box<dyn uart::Driver>>,
}

impl<P: Platform> Manager<P> {
    fn new(module: ModuleBase<P>) -> Self {
        Self {
            module,
            uart: Default::default(),
            registed: Default::default(),
            registers: Default::default(),
        }
    }

    pub async fn init_stdout(&mut self) {
        if let Some(stdout) = self.probe_stdout().await {
            self.module.stdout.set(stdout);
        }
    }

    async fn init(&mut self) {
        self.probe_uart().await;
    }

    async fn probe_stdout(&mut self) -> Option<io::BoxWrite> {
        let fdt = get_device_tree().expect("no device tree found!");
        let chosen = fdt.chosen().ok()?;
        let stdout = chosen.stdout()?;
        let node = stdout.node();
        if let Some(d) = self.node_probe_uart(node).await {
            return Some(d);
        }

        None
    }

    async fn probe_uart(&mut self) -> Option<io::BoxWrite> {
        let fdt = get_device_tree()?;
        for node in fdt.all_nodes() {
            if let Some(d) = self.node_probe_uart(node).await {
                self.uart.push(d);
            }
        }
        None
    }

    async fn node_probe_uart(&mut self, node: FdtNode<'_, '_>) -> Option<uart::BoxDriver> {
        let caps = node.compatible()?;
        for one in caps.all() {
            for register in self.registers.values() {
                let name = &register.name;
                if self.registed.contains(name) {
                    continue;
                }

                if register.compatible_matched(one) {
                    if let RegisterKind::Uart(ref register) = register.kind {
                        let reg = node.reg().next()?;
                        let start = (reg.starting_address as usize).into();
                        let size = reg.size?;
                        let reg_base = self.module.memory.iomap(start, size);

                        let clock_freq = if let Some(clk) = get_uart_clk(&node) {
                            clk
                        } else {
                            continue;
                        };

                        let config = uart::Config {
                            reg: reg_base,
                            baud_rate: 115200,
                            clock_freq: clock_freq as _,
                            data_bits: uart::DataBits::Bits8,
                            stop_bits: uart::StopBits::STOP1,
                            parity: uart::Parity::None,
                        };
                        let uart = register.probe(config).await.ok()?;
                        self.registed.insert(name.clone());
                        return Some(uart);
                    }
                }
            }
        }
        None
    }
}

fn get_uart_clk(uart_node: &FdtNode<'_, '_>) -> Option<u64> {
    let fdt = get_device_tree()?;
    let clk = uart_node.property("clocks")?;
    for phandle in clk.iter_cell_size(CellSize::One) {
        if let Some(node) = fdt.find_phandle(phandle as _) {
            return node.property("clock-frequency")?.as_usize().map(|c| c as _);
        }
    }
    None
}
