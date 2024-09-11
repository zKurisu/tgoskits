use crate::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

pub struct Module<T> {
    inner: RwLock<Option<T>>,
}

impl<T> Module<T> {
    pub const fn uninit() -> Self {
        Self {
            inner: RwLock::new(None),
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, Option<T>> {
        self.inner.write()
    }

    pub fn read(&self) -> RwLockReadGuard<'_, Option<T>> {
        self.inner.read()
    }
}
