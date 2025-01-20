use alloc::string::{String, ToString};
use tcb::{Pid, TaskControlBlock, set_current};

mod schedule;
mod tcb;

pub use schedule::suspend;
pub use tcb::current;

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

impl TaskConfig {
    pub fn new(name: impl ToString) -> Self {
        Self {
            name: name.to_string(),
            priority: 0,
            stack_size: 2 * 1024 * 1024,
        }
    }
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
        name: "Task0".into(),
        priority: 0,
        stack_size: 0,
    })
    .unwrap();
    set_current(&task);
}

pub fn wake_up_in_irq(_pid: Pid) {}
