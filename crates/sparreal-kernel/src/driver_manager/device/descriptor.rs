use alloc::{string::String, vec::Vec};
use core::fmt::Debug;

use super::irq::IrqInfo;

custom_id!(DeviceId, u64);
custom_id!(DriverId, u64);

#[derive(Default, Debug, Clone)]
pub struct Descriptor {
    pub device_id: DeviceId,
    pub driver_id: DriverId,
    pub name: String,
    pub irq_configs: Vec<IrqInfo>,
}

macro_rules! impl_driver_id_for {
    ($t:ty) => {
        impl From<$t> for DriverId {
            fn from(value: $t) -> Self {
                Self(value as _)
            }
        }
    };
}

impl_driver_id_for!(usize);
impl_driver_id_for!(u32);
