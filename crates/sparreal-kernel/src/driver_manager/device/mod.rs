use alloc::{
    string::{String, ToString},
    sync::Arc,
};
use core::{
    fmt::Debug,
    ops::{Deref, DerefMut},
};

mod descriptor;

pub use descriptor::*;
use spin::Mutex;

use super::DriverError;

#[derive(Clone)]
pub struct Device<T> {
    pub descriptor: Descriptor,
    data: Arc<Mutex<BorrowInfo<T>>>,
}

impl<T> Debug for Device<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Device")
            .field("device_id", &self.descriptor.device_id)
            .field("driver_id", &self.descriptor.driver_id)
            .field("name", &self.descriptor.name)
            .finish()
    }
}

struct BorrowInfo<T> {
    who: String,
    data: Option<T>,
}

impl<T> Device<T> {
    pub fn new(descriptor: Descriptor, data: T) -> Self {
        Device {
            descriptor,
            data: Arc::new(Mutex::new(BorrowInfo {
                who: String::new(),
                data: None,
            })),
        }
    }

    pub fn try_use_by(&self, who: impl ToString) -> Result<BorrowGuard<T>, DriverError> {
        let descriptor = self.descriptor.clone();
        let lock = self.data.clone();
        let mut g = self.data.lock();

        let driver = g
            .data
            .take()
            .ok_or(DriverError::UsedByOthers(g.who.clone()))?;
        g.who = who.to_string();

        Ok(BorrowGuard {
            data: Some(driver),
            lock,
            descriptor,
        })
    }
}

pub struct BorrowGuard<T> {
    pub descriptor: Descriptor,
    data: Option<T>,
    lock: Arc<Mutex<BorrowInfo<T>>>,
}

impl<T> Deref for BorrowGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.data.as_ref().unwrap()
    }
}

impl<T> DerefMut for BorrowGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data.as_mut().unwrap()
    }
}

impl<T> Drop for BorrowGuard<T> {
    fn drop(&mut self) {
        let mut g = self.lock.lock();
        g.data = Some(self.data.take().unwrap());
    }
}
