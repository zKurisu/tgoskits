use alloc::sync::Arc;
use core::ops::{Deref, DerefMut};

use err::DriverError;
use spin::Mutex;

pub mod err;
pub mod interrupt_controller;

#[derive(Clone)]
pub struct DriverMutex<T> {
    data: Arc<Mutex<Option<T>>>,
}

impl<T> DriverMutex<T> {
    pub fn new(data: T) -> Self {
        DriverMutex {
            data: Arc::new(Mutex::new(Some(data))),
        }
    }

    pub fn get(&self) -> Result<BorrowGuard<T>, DriverError> {
        let lock = self.data.clone();
        let mut g = self.data.try_lock().ok_or(DriverError::UsedByOthers)?;
        let driver = g.take().ok_or(DriverError::UsedByOthers)?;
        Ok(BorrowGuard {
            data: Some(driver),
            lock,
        })
    }
}

pub struct BorrowGuard<T> {
    data: Option<T>,
    lock: Arc<Mutex<Option<T>>>,
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
        *g = Some(self.data.take().unwrap());
    }
}
