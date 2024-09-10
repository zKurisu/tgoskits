use alloc::boxed::Box;

use crate::DriverResult;

pub trait Driver: super::DriverGeneric {
    
}

pub trait Register: super::RegisterGeneric {
    fn probe(&self, config: Config) -> DriverResult<Box<dyn Driver>>;
}

pub struct Config {}
