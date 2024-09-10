use alloc::{boxed::Box, string::String, vec::Vec};
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

    pub fn register_uart(&self, register: impl DriverRegisterUart + 'static) {
        self.inner.write().register_uart.push(Box::new(register));
    }
}

struct Manager {
    register_uart: Vec<Box<dyn DriverRegisterUart>>,
}

impl Manager {
    const fn new() -> Self {
        Self {
            register_uart: Vec::new(),
        }
    }

    fn init(&mut self) {
        self.probe_stdout();
    }

    fn probe_stdout(&mut self) -> Option<()> {
        let fdt = get_device_tree().expect("no device tree found!");
        let chosen = fdt.chosen().ok()?;
        let stdout = chosen.stdout()?;
        let node = stdout.node();
        let caps = node.compatible()?;
        let regs = node.reg();         

        for one in caps.all() {
            for register in &self.register_uart {
                if register.compatible_matched(one) {
                    let uart = register.probe();
                }
            }
        }

        Some(())
    }
}

pub trait Driver {}

pub trait DriverUart: Driver {}

pub trait DriverRegisterUart: DriverRegister {
    fn probe(&self) -> Box<dyn DriverUart>;
}

pub trait DriverRegister {
    fn compatible(&self) -> Vec<String>;

    fn compatible_matched(&self, compatible: &str) -> bool {
        for one in self.compatible() {
            if one.as_str().eq(compatible) {
                return true;
            }
        }
        false
    }
}
