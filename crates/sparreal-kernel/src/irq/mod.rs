use core::{cell::UnsafeCell, error::Error};

use alloc::{boxed::Box, collections::btree_map::BTreeMap, vec::Vec};
use driver_interface::interrupt_controller::*;
use log::{debug, error};
use spin::Mutex;

use crate::{
    driver_manager::{self, device::DriverId},
    globals::{self, cpu_global, global_val},
    platform::{self},
    platform_if::PlatformImpl,
};
pub use driver_manager::device::irq::IrqInfo;

#[derive(Default)]
pub struct CpuIrqChips(BTreeMap<DriverId, Chip>);

pub type IrqHandler = dyn Fn(IrqId) -> IrqHandleResult;

pub struct Chip {
    device: HardwareCPU,
    mutex: Mutex<()>,
    handlers: UnsafeCell<BTreeMap<IrqId, Box<IrqHandler>>>,
}

unsafe impl Send for Chip {}
unsafe impl Sync for Chip {}

pub fn enable_all() {
    PlatformImpl::irq_all_enable();
}

pub(crate) fn init_main_cpu() {
    match &global_val().platform_info {
        crate::globals::PlatformInfoKind::DeviceTree(fdt) => {
            if let Err(e) = driver_manager::init_irq_chips_by_fdt(fdt.get_addr()) {
                error!("{}", e);
            }
        }
    }

    init_current_cpu();
}

pub(crate) fn init_current_cpu() {
    let chip = driver_manager::use_irq_chips_by("Kernel IRQ init");
    let g = unsafe { globals::cpu_global_mut() };

    for c in chip {
        let id = c.descriptor.driver_id;
        let device = c.current_cpu_setup();
        debug!(
            "[{}]({id:?}) Init cpu: {:?}",
            c.descriptor.name,
            platform::cpu_id(),
        );

        g.irq_chips.0.insert(id, Chip {
            device,
            mutex: Mutex::new(()),
            handlers: UnsafeCell::new(Default::default()),
        });
    }
}

pub enum IrqHandleResult {
    Handled,
    None,
}

fn chip(id: DriverId) -> &'static Chip {
    globals::cpu_global()
        .irq_chips
        .0
        .get(&id)
        .unwrap_or_else(|| panic!("irq chip {:?} not found", id))
}

pub fn fdt_parse_config(
    irq_parent: DriverId,
    prop_interrupts: &[u32],
) -> Result<IrqConfig, Box<dyn Error>> {
    chip(irq_parent).device.parse_fdt_config(prop_interrupts)
}

pub struct IrqRegister {
    pub param: IrqParam,
    pub handler: Box<IrqHandler>,
    pub priority: Option<usize>,
    pub cpu_list: Vec<CpuId>,
}

impl IrqRegister {
    pub fn register(self) {
        let irq = self.param.cfg.irq;
        let irq_parent = self.param.irq_chip;
        debug!("Enable irq {:?} on chip {:?}", irq, irq_parent);

        let chip = chip(irq_parent);
        chip.register_handle(irq, self.handler);

        let c = &chip.device;
        if let Some(p) = self.priority {
            c.set_priority(irq, p);
        } else {
            c.set_priority(irq, 0);
        }

        if !self.cpu_list.is_empty() {
            c.set_bind_cpu(irq, &self.cpu_list);
        }

        c.set_trigger(irq, self.param.cfg.trigger);
        c.irq_enable(irq);
    }

    pub fn priority(mut self, priority: usize) -> Self {
        self.priority = Some(priority);
        self
    }

    pub fn cpu_list(mut self, cpu_list: Vec<CpuId>) -> Self {
        self.cpu_list = cpu_list;
        self
    }
}

impl Chip {
    fn register_handle(&self, irq: IrqId, handle: Box<IrqHandler>) {
        let g = NoIrqGuard::new();
        let gm = self.mutex.lock();
        unsafe { &mut *self.handlers.get() }.insert(irq, handle);
        drop(gm);
        drop(g);
    }

    fn unregister_handle(&self, irq: IrqId) {
        let g = NoIrqGuard::new();
        let gm = self.mutex.lock();
        unsafe { &mut *self.handlers.get() }.remove(&irq);
        drop(gm);
        drop(g);
    }

    fn handle_irq(&self) -> Option<()> {
        let irq = self.device.get_and_acknowledge_interrupt()?;

        if let Some(handler) = unsafe { &mut *self.handlers.get() }.get(&irq) {
            (handler)(irq);
        }
        self.device.end_interrupt(irq);
        Some(())
    }
}

pub struct NoIrqGuard {
    is_enabled: bool,
}

impl NoIrqGuard {
    pub fn new() -> Self {
        let is_enabled = PlatformImpl::irq_all_is_enabled();
        PlatformImpl::irq_all_disable();
        Self { is_enabled }
    }
}

impl Default for NoIrqGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for NoIrqGuard {
    fn drop(&mut self) {
        if self.is_enabled {
            enable_all();
        }
    }
}

pub fn handle_irq() {
    for chip in cpu_global().irq_chips.0.values() {
        chip.handle_irq();
    }
}

#[derive(Debug, Clone)]
pub struct IrqParam {
    pub irq_chip: DriverId,
    pub cfg: IrqConfig,
}

impl IrqParam {
    pub fn register_builder(
        &self,
        handler: impl Fn(IrqId) -> IrqHandleResult + 'static,
    ) -> IrqRegister {
        IrqRegister {
            param: self.clone(),
            handler: Box::new(handler),
            priority: None,
            cpu_list: Vec::new(),
        }
    }
}

pub fn unregister_irq(irq: IrqId) {
    for chip in cpu_global().irq_chips.0.values() {
        chip.unregister_handle(irq);
    }
}
