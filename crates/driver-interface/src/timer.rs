use core::time::Duration;

use alloc::boxed::Box;
use futures::future::LocalBoxFuture;

use crate::{io, irq::IrqConfig, DriverResult};

pub trait Driver: super::DriverGeneric {
    fn set_one_shot(&self, delay: Duration);
}

pub type BoxDriver = Box<dyn Driver>;
