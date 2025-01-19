use alloc::string::String;
use tcb::{TaskControlBlock, set_current};

use crate::platform::wait_for_interrupt;

mod schedule;
mod tcb;

#[derive(Debug, Clone)]
pub enum TaskError {
    NoMemory,
}

#[derive(Debug, Clone)]
pub struct TaskConfig {
    pub name: String,
    pub priority: usize,
    pub stack_size: usize,
}

pub fn spawn_with_config<F>(f: F, config: TaskConfig) -> Result<(), TaskError>
where
    F: FnOnce() + Send + 'static,
{
    let task = TaskControlBlock::new(f, config)?;

    tcb::current().switch_to(&task);

    Ok(())
}

pub fn init() {
    let task = TaskControlBlock::new(|| {}, TaskConfig {
        name: "Main".into(),
        priority: 0,
        stack_size: 0,
    })
    .unwrap();
    set_current(&task);
}
