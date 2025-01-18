use alloc::{
    string::{String, ToString},
    sync::Arc,
};
use core::{
    cell::UnsafeCell,
    fmt::Debug,
    ops::{Deref, DerefMut},
};

mod descriptor;
pub mod irq;
pub mod timer;

pub use descriptor::*;
use spin::Mutex;

use super::DriverError;

#[derive(Clone)]
pub struct Device<T> {
    pub descriptor: Descriptor,
    lock: Arc<Mutex<Lock>>,
    data: Arc<UnsafeCell<T>>,
}

unsafe impl<T> Send for Device<T> {}

impl<T> Debug for Device<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Device")
            .field("device_id", &self.descriptor.device_id)
            .field("driver_id", &self.descriptor.driver_id)
            .field("name", &self.descriptor.name)
            .field("irq_configs", &self.descriptor.irq)
            .finish()
    }
}

struct Lock {
    who_using: Option<String>,
}

impl<T> Device<T> {
    pub fn new(descriptor: Descriptor, data: T) -> Self {
        Device {
            descriptor,
            data: Arc::new(UnsafeCell::new(data)),
            lock: Arc::new(Mutex::new(Lock { who_using: None })),
        }
    }

    pub fn try_use_by(&self, who: impl ToString) -> Result<BorrowGuard<T>, DriverError> {
        let descriptor = self.descriptor.clone();
        let lock = self.lock.clone();
        let mut g = self.lock.lock();
        if let Some(ref who) = g.who_using {
            return Err(DriverError::UsedByOthers(who.to_string()));
        }
        g.who_using = Some(who.to_string());
        let data = self.data.clone();
        Ok(BorrowGuard {
            lock,
            descriptor,
            data,
        })
    }

    pub fn spin_try_use(&self, who: impl ToString) -> BorrowGuard<T> {
        loop {
            if let Ok(g) = self.try_use_by(who.to_string()) {
                return g;
            }
        }
    }

    /// 强制获取设备
    ///
    /// # Safety
    /// 一般用于中断处理中
    pub unsafe fn force_use(&self) -> *mut T {
        self.data.get()
    }
}

impl<T: Sync> Deref for Device<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.data.get() }
    }
}

pub struct BorrowGuard<T> {
    pub descriptor: Descriptor,
    lock: Arc<Mutex<Lock>>,
    data: Arc<UnsafeCell<T>>,
}

impl<T> Deref for BorrowGuard<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.data.get() }
    }
}

impl<T> DerefMut for BorrowGuard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.data.get() }
    }
}

impl<T> Drop for BorrowGuard<T> {
    fn drop(&mut self) {
        let mut g = self.lock.lock();
        g.who_using = None;
    }
}
