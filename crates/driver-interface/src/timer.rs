use alloc::boxed::Box;
use futures::future::LocalBoxFuture;

use crate::{io, irq::IrqConfig, DriverResult};

pub trait Driver: super::DriverGeneric {}

pub type BoxDriver = Box<dyn Driver>;
