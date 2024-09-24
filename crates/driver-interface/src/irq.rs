use core::ptr::NonNull;

use alloc::boxed::Box;
use futures::future::LocalBoxFuture;

use crate::DriverResult;

pub trait Driver: super::DriverGeneric {}

pub type BoxDriver = Box<dyn Driver>;
pub type BoxRegister = Box<dyn Register>;

pub trait Register: Send + Sync + 'static {
    fn probe<'a>(&self, config: Config) -> LocalBoxFuture<'a, DriverResult<BoxDriver>>;
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



