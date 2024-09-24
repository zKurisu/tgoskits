use core::fmt::Write;

use alloc::{
    collections::btree_map::BTreeMap,
    string::{String, ToString},
    sync::Arc,
};
use driver_interface::*;
use flat_device_tree::node::{CellSize, FdtNode};
use log::{debug, info};

use super::{device_tree::get_device_tree, DriverLocked};
use crate::{
    driver::DriverKind,
    mem::mmu::iomap,
    stdout::{self, set_stdout, DriverWrite, EarlyDebugWrite},
    sync::RwLock,
};

static MANAGER: RwLock<Option<DriverManager>> = RwLock::new(None);
pub fn driver_manager() -> DriverManager {
    let d = MANAGER.read();
    match d.as_ref() {
        Some(dm) => dm.clone(),
        None => {
            drop(d);
            let mut g = MANAGER.write();
            match g.as_mut() {
                Some(d) => d.clone(),
                None => {
                    let d = DriverManager::new();
                    *g = Some(d.clone());
                    d
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct DriverManager {
    inner: Arc<RwLock<Manager>>,
}

impl DriverManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Manager::new())),
        }
    }

    pub async fn init(&self) {
        {
            let mut g = self.inner.write();
            g.init_irq().await;
        }

        self.init_stdout().await;
        {
            let mut g = self.inner.write();
            g.init_drivers().await;
        }
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

    pub fn get_driver(&self, name: &str) -> Option<DriverLocked> {
        self.inner.read().get_driver(name)
    }

    async fn init_stdout(&self) {
        let stdout = {
            let mut g = self.inner.write();
            g.probe_stdout().await
        };
        if let Some(out) = stdout {
            let name = out.name();
            info!("stdout: {}", name);
            set_stdout(out);
            info!("stdout init success!");
        }
    }
}

pub(super) struct Manager {
    pub(super) registers: BTreeMap<String, Register>,
    pub(super) drivers: BTreeMap<String, DriverLocked>,
}

impl Manager {
    fn new() -> Self {
        Self {
            drivers: Default::default(),
            registers: Default::default(),
        }
    }

    async fn init_drivers(&mut self) {
        debug!("Driver manager init start!");
        self.probe_all().await;
    }

    pub async fn probe_all(&mut self) {
        self.probe_uart().await;
    }

    pub(super) fn add_driver(&mut self, name: String, kind: DriverKind) -> DriverLocked {
        let driver = DriverLocked::new(name.clone(), kind);
        self.drivers.insert(name, driver.clone());
        driver
    }
    pub fn get_driver(&self, name: &str) -> Option<DriverLocked> {
        self.drivers.get(name).cloned()
    }
    async fn probe_stdout(&mut self) -> Option<DriverWrite> {
        let fdt = get_device_tree().expect("no device tree found!");
        let chosen = fdt.chosen().ok()?;
        let stdout = chosen.stdout()?;
        let node = stdout.node();
        if let Some(d) = self.node_probe_uart(node).await {
            let name = node.name.to_string();
            let driver = self.add_driver(name.clone(), DriverKind::Uart(d));
            return Some(DriverWrite::new(&driver));
        }
        None
    }

    async fn probe_uart(&mut self) -> Option<()> {
        let fdt = get_device_tree()?;
        for node in fdt.all_nodes() {
            self.node_register_uart(node).await;
        }
        Some(())
    }
    async fn node_register_uart(&mut self, node: FdtNode<'_, '_>) {
        if self.drivers.contains_key(node.name) {
            return;
        }

        if let Some(d) = self.node_probe_uart(node).await {
            self.add_driver(node.name.to_string(), DriverKind::Uart(d));
        }
    }

    async fn node_probe_uart(&mut self, node: FdtNode<'_, '_>) -> Option<uart::BoxDriver> {
        let caps = node.compatible()?;
        for one in caps.all() {
            for register in self.registers.values() {
                if register.compatible_matched(one) {
                    if let RegisterKind::Uart(ref register) = register.kind {
                        info!("Probe {} - uart: {}", node.name, one);
                        let reg = node.reg_fix().next()?;
                        let start = (reg.starting_address as usize).into();
                        let size = reg.size?;

                        info!("    @{} size: {:#X}", start, size);

                        let reg_base = iomap(start, size);

                        let clock_freq = if let Some(clk) = get_uart_clk(&node) {
                            clk
                        } else {
                            0
                        };

                        info!("    clk: {}", clock_freq);

                        let config = uart::Config {
                            reg: reg_base,
                            baud_rate: 115200,
                            clock_freq,
                            data_bits: uart::DataBits::Bits8,
                            stop_bits: uart::StopBits::STOP1,
                            parity: uart::Parity::None,
                        };
                        let uart = register.probe(config).await.ok()?;

                        info!("    probe success!");

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
