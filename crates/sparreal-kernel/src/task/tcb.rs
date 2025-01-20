use core::{
    alloc::Layout,
    fmt::Debug,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

use alloc::{boxed::Box, string::String};
use log::debug;

use crate::{platform_if::PlatformImpl, task::schedule::*};

use super::{TaskConfig, TaskError};

const TCB_ALIGN: usize = 16;

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct TaskControlBlock(*mut u8);

unsafe impl Send for TaskControlBlock {}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Pid(usize);

impl Pid {
    pub fn new() -> Self {
        static ITER: AtomicUsize = AtomicUsize::new(0);
        Self(ITER.fetch_add(1, Ordering::SeqCst))
    }
}

impl Default for Pid {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for Pid {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "{:?}", self.0)
    }
}

impl TaskControlBlock {
    pub(super) fn new<F>(entry: F, config: TaskConfig) -> Result<Self, TaskError>
    where
        F: FnOnce() + Send + 'static,
    {
        let entry_box = Box::new(entry);

        let buffer = NonNull::new(unsafe {
            alloc::alloc::alloc_zeroed(
                Layout::from_size_align(Self::tcb_size(config.stack_size), TCB_ALIGN).unwrap(),
            )
        })
        .ok_or(TaskError::NoMemory)?;

        let pid = Pid::new();

        let pc;

        unsafe {
            let task_info = &mut *(buffer.as_ptr() as *mut TaskInfo);
            task_info.pid = pid;
            task_info.stack_size = config.stack_size;
            task_info.priority = config.priority;
            task_info.name = config.name;
            task_info.state = TaskState::Idle;
            task_info.entry = Some(entry_box);

            pc = task_entry as usize as _;
        }

        let task = Self(buffer.as_ptr());

        unsafe {
            let stack_top = task.stack().as_ptr().add(config.stack_size);

            debug!(
                "New task [{:?}], stack_top: {:p}",
                task.info().name,
                stack_top
            );

            PlatformImpl::cpu_context_init(
                task.cpu_context_ptr(),
                pc,
                task.stack().as_ptr().add(config.stack_size),
            );
        }
        Ok(task)
    }

    fn tcb_size(stack_size: usize) -> usize {
        size_of::<TaskInfo>() + stack_size + PlatformImpl::cpu_context_size()
    }

    pub fn info(&self) -> &TaskInfo {
        unsafe { &*(self.0 as *mut TaskInfo) }
    }

    pub(super) fn info_mut(&mut self) -> &mut TaskInfo {
        unsafe { &mut *(self.0 as *mut TaskInfo) }
    }

    pub(super) fn stack(&self) -> &[u8] {
        let stack_size = self.info().stack_size;
        unsafe { core::slice::from_raw_parts_mut(self.0.add(size_of::<TaskInfo>()), stack_size) }
    }

    pub(super) unsafe fn drop(self) {
        let info = self.info();

        let size = Self::tcb_size(info.stack_size);

        unsafe {
            alloc::alloc::dealloc(self.0, Layout::from_size_align_unchecked(size, TCB_ALIGN))
        };
    }

    fn cpu_context_ptr(&self) -> *mut u8 {
        unsafe {
            self.0
                .add(size_of::<TaskInfo>())
                .add(self.info().stack_size)
        }
    }

    fn addr(&self) -> *mut u8 {
        self.0 as _
    }

    pub(super) fn switch_to(&self, next: &TaskControlBlock) {
        debug!("switch {} -> {}", self.info().name, next.info().name);
        set_current(next);
        match self.info().state {
            TaskState::Stopped => finished_push(*self),
            _ => idle_push(*self),
        }

        unsafe {
            PlatformImpl::cpu_context_switch(self.cpu_context_ptr(), next.cpu_context_ptr());
        }
    }

    #[allow(unused)]
    pub fn sp(&self) -> usize {
        unsafe { PlatformImpl::cpu_context_sp(self.cpu_context_ptr()) }
    }
}

pub struct TaskInfo {
    pub pid: Pid,
    pub name: String,
    pub priority: usize,
    pub stack_size: usize,
    pub entry: Option<Box<dyn FnOnce()>>,
    pub state: TaskState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskState {
    Idle,
    Running,
    Suspend,
    Stopped,
}

extern "C" fn task_entry() -> ! {
    let mut task = current();
    let task_mut = task.info_mut();
    if let Some(entry) = task_mut.entry.take() {
        entry();
        task_mut.state = TaskState::Stopped;
    }
    schedule();
    unreachable!("task exited!");
}

pub fn current() -> TaskControlBlock {
    unsafe {
        let ptr = PlatformImpl::get_current_tcb_addr();
        TaskControlBlock(ptr as _)
    }
}

pub fn set_current(tcb: &TaskControlBlock) {
    unsafe {
        PlatformImpl::set_current_tcb_addr(tcb.addr());
    }
}
