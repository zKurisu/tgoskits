#![no_std]

use core::{fmt::Display, ptr::NonNull};

use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

extern crate alloc;

pub use futures::future::BoxFuture;
use futures::future::LocalBoxFuture;
use irq::Trigger;

pub mod io;
pub mod irq;
pub mod timer;
pub mod uart;

pub type DriverResult<T = ()> = Result<T, DriverError>;

pub trait DriverGeneric: Send + Sync + 'static {}

#[derive(Clone)]
pub struct Register {
    pub name: String,
    pub compatible: Vec<&'static str>,
    pub kind: DriverKind,
    pub probe: Arc<dyn Probe>,
}
impl Register {
    pub fn new(
        name: &str,
        compatible: Vec<&'static str>,
        kind: DriverKind,
        probe: impl Probe,
    ) -> Self {
        Register {
            name: name.to_string(),
            compatible,
            kind,
            probe: Arc::new(probe),
        }
    }

    pub fn compatible_matched(&self, compatible: &str) -> bool {
        self.compatible.contains(&compatible)
    }
}

#[derive(Default, Clone)]
pub struct ProbeConfig {
    pub id: DeviceId,
    pub reg: Vec<NonNull<u8>>,
    pub irq: Vec<IrqProbeConfig>,
    pub clock_freq: Vec<u64>,
}

#[derive(Clone)]
pub struct IrqProbeConfig {
    pub irq: usize,
    pub trigger: Trigger,
}

pub trait Probe: Send + Sync + 'static {
    fn probe<'a>(&self, config: ProbeConfig) -> LocalBoxFuture<'a, DriverResult<DriverSpecific>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DriverKind {
    InteruptChip,
    Uart,
    Timer,
}

pub enum DriverSpecific {
    Uart(uart::BoxDriver),
    InteruptChip(irq::BoxDriver),
    Timer(timer::BoxDriver),
}

#[derive(Debug)]
pub enum DriverError {
    Init(String),
    NotFound,
    NoMemory,
}

#[repr(transparent)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeviceId(u64);

impl Into<u64> for DeviceId {
    fn into(self) -> u64 {
        self.0
    }
}

impl From<u64> for DeviceId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl Display for DeviceId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}
