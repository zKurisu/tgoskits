use core::ptr::NonNull;

use alloc::{boxed::Box, vec::Vec};
use futures::future::LocalBoxFuture;

use crate::DriverResult;

pub trait Driver: super::DriverGeneric {
    fn get_and_acknowledge_interrupt(&self) -> Option<usize>;
    fn end_interrupt(&self, irq_id: usize);
    fn irq_max_size(&self) -> usize;
    fn enable_irq(&mut self, config: IrqConfig);
    fn disable_irq(&mut self, irq_id: usize);
    fn current_cpu_setup(&self);
}

pub type BoxDriver = Box<dyn Driver>;
pub type BoxRegister = Box<dyn Register>;

pub trait Register: Send + Sync + 'static {
    fn probe<'a>(&self, config: Config) -> LocalBoxFuture<'a, DriverResult<BoxDriver>>;
}

#[derive(Debug, Clone)]
pub struct IrqConfig {
    pub irq_id: usize,
    pub trigger: Trigger,
    pub priority: usize,
    pub cpu_list: Vec<usize>,
}

pub struct Config {
    pub reg1: NonNull<u8>,
    pub reg2: NonNull<u8>,
}

unsafe impl Send for Config {}
unsafe impl Sync for Config {}

/// The trigger configuration for an interrupt.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Trigger {
    /// The interrupt is edge triggered.
    Edge,
    /// The interrupt is level triggered.
    Level,
}
