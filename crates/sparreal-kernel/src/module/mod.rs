use crate::{
    mem::MemoryManager,
    stdout::Stdout,
    sync::{RwLock, RwLockReadGuard, RwLockWriteGuard},
    time::Time,
    Platform,
};

pub struct ModuleBase<P: Platform> {
    pub memory: MemoryManager<P>,
    pub time: Time<P>,
    pub stdout: Stdout,
}

impl<P: Platform> Clone for ModuleBase<P> {
    fn clone(&self) -> Self {
        Self {
            memory: self.memory.clone(),
            time: self.time.clone(),
            stdout: self.stdout.clone(),
        }
    }
}
