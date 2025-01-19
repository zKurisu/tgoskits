use alloc::collections::vec_deque::VecDeque;
use spin::Mutex;

use crate::platform_if::PlatformImpl;

use super::tcb::{TaskControlBlock, TaskState, current};

static IDLE: Mutex<VecDeque<TaskControlBlock>> = Mutex::new(VecDeque::new());
static FINISHED: Mutex<VecDeque<TaskControlBlock>> = Mutex::new(VecDeque::new());

pub fn schedule() {
    let idle = idle_pop();
    if let Some(idle) = idle {
        current().switch_to(&idle);
    } else {
        loop {
            PlatformImpl::wait_for_interrupt();
        }
    }
}

pub fn idle_push(tcb: TaskControlBlock) {
    IDLE.lock().push_back(tcb);
}

pub fn idle_pop() -> Option<TaskControlBlock> {
    IDLE.lock().pop_front()
}

pub fn finished_push(tcb: TaskControlBlock) {
    FINISHED.lock().push_back(tcb);
}

pub fn suspend() {
    let mut current = current();
    current.info_mut().state = TaskState::Suspend;
    schedule();
}
