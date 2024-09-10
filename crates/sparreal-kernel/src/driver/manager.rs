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
        let fdt = get_device_tree().expect("no device tree found!");
        let chosen = fdt.chosen().unwrap();
        let stdout = chosen.stdout().unwrap();
        let params = stdout.params();
        let node = stdout.node();
        let name = node.name;
        let caps = node.compatible().unwrap();
        for one in caps.all() {
            for register in &self.register_uart {
                if register.compatible_matched(one){
                    let a = 0;
                }
            }
        }

        let regs = node.reg();

        for reg in regs {
            let base = reg.starting_address;
            let size = reg.size;
        }
    }
}

pub trait DriverRegisterUart: Driver {}

pub trait Driver {
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
