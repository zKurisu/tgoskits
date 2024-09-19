use crate::{mem::MemoryManager, time::Time, Platform};

pub struct ModuleBase<P: Platform> {
    pub memory: MemoryManager<P>,
    pub time: Time<P>,
}

impl<P: Platform> Clone for ModuleBase<P> {
    fn clone(&self) -> Self {
        Self {
            memory: self.memory.clone(),
            time: self.time.clone(),
        }
    }
}
