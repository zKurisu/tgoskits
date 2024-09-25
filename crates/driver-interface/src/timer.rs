use core::ptr::NonNull;

use alloc::{boxed::Box, vec::Vec};
use futures::future::LocalBoxFuture;

use crate::{io, irq::IrqConfig, DriverResult};

pub trait Driver: super::DriverGeneric {}

pub type BoxDriver = Box<dyn Driver>;
pub type BoxRegister = Box<dyn Register>;

pub trait Register: Send + Sync + 'static {
    fn probe<'a>(&self, config: Config) -> LocalBoxFuture<'a, DriverResult<BoxDriver>>;
}

pub struct Config {
    pub reg: Option<NonNull<u8>>,
    pub interrupt: Vec<IrqConfig>,
}

unsafe impl Send for Config {}
unsafe impl Sync for Config {}
