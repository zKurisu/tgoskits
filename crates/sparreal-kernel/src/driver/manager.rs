use core::ptr::NonNull;

use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, sync::Arc, vec::Vec};
use driver_interface::*;
use flat_device_tree::standard_nodes::Chosen;
use uart::Driver;

use crate::{executor, mem::MemoryManager, stdout, sync::RwLock, Platform};

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
    pub fn new(mem: MemoryManager<P>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Manager::new(mem))),
        }
    }

    pub async fn init(&self) {
        self.inner.write().init().await;
    }

    pub fn register_uart(&self, register: impl uart::Register + 'static) {
        self.inner.write().register_uart.push(Box::new(register));
    }
}

struct Manager<P: Platform> {
    mem: MemoryManager<P>,
    register_uart: Vec<Box<dyn uart::Register>>,
    uart: BTreeMap<String, Box<dyn uart::Driver>>,
}

impl<P: Platform> Manager<P> {
    fn new(mem: MemoryManager<P>) -> Self {
        Self {
            mem,
            register_uart: Vec::new(),
            uart: BTreeMap::new(),
        }
    }

    async fn init(&mut self) {
        if let Some(stdout) = self.probe_stdout().await {
            stdout::set_stdout(stdout);
        }
    }

    async fn probe_stdout(&mut self) -> Option<Box<dyn uart::Driver>> {
        let fdt = get_device_tree().expect("no device tree found!");
        let chosen = fdt.chosen().ok()?;
        let stdout = chosen.stdout()?;
        let node = stdout.node();
        let caps = node.compatible()?;

        for one in caps.all() {
            for register in &self.register_uart {
                if register.compatible_matched(one) {
                    let reg = node.reg().next()?;
                    let start = (reg.starting_address as usize).into();
                    let size = reg.size?;
                    let reg_base = self.mem.iomap(start, size);

                    let config = uart::Config {
                        reg: reg_base,
                        baud_rate: 115200,
                        clock_freq: 1,
                        data_bits: uart::DataBits::Bits8,
                        stop_bits: uart::StopBits::STOP1,
                        parity: uart::Parity::None,
                    };
                    let uart = register.probe(config).await.ok()?;
                    return Some(uart);
                }
            }
        }

        None
    }
}
