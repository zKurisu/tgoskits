use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, vec::Vec};
use driver_interface::*;
use flat_device_tree::standard_nodes::Chosen;

use crate::sync::RwLock;

use super::device_tree::get_device_tree;

pub struct DriverManager {
    inner: RwLock<Manager>,
}

impl DriverManager {
    pub const fn new() -> Self {
        Self {
            inner: RwLock::new(Manager::new()),
        }
    }

    pub fn init(&self) {
        self.inner.write().init();
    }

    pub fn register_uart(&self, register: impl uart::Register + 'static) {
        self.inner.write().register_uart.push(Box::new(register));
    }
}

struct Manager {
    register_uart: Vec<Box<dyn uart::Register>>,
    uart: BTreeMap<String, Box<dyn uart::Driver>>,
}

impl Manager {
    const fn new() -> Self {
        Self {
            register_uart: Vec::new(),
            uart: BTreeMap::new(),
        }
    }

    fn init(&mut self) {
        if let Some(stdout) = self.probe_stdout() {
            self.uart.insert(stdout.name(), stdout);
        }
    }

    fn probe_stdout(&mut self) -> Option<Box<dyn uart::Driver>> {
        let fdt = get_device_tree().expect("no device tree found!");
        let chosen = fdt.chosen().ok()?;
        let stdout = chosen.stdout()?;
        let node = stdout.node();
        let caps = node.compatible()?;
        let regs = node.reg();

        for one in caps.all() {
            for register in &self.register_uart {
                if register.compatible_matched(one) {
                    let config = uart::Config {};
                    let uart = register.probe(config).ok()?;
                    return Some(uart);
                }
            }
        }

        None
    }
}
