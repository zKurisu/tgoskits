use driver_interface::interrupt_controller;

use super::DriverMutex;

pub struct Device {
    pub id: u64,
    pub driver: DriverMutex<interrupt_controller::BoxedDriver>,
}

impl From<interrupt_controller::BoxedDriver> for Device {
    fn from(value: interrupt_controller::BoxedDriver) -> Self {
        Self {
            id: 0,
            driver: DriverMutex::new(value),
        }
    }
}
