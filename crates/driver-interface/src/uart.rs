use alloc::boxed::Box;
use futures::future::BoxFuture;

use crate::DriverResult;

pub trait Driver: super::DriverGeneric {}

pub trait Register: super::RegisterGeneric {
    fn probe(&self, config: Config) -> BoxFuture<DriverResult<Box<dyn Driver>>>;
}

pub struct Config {}
