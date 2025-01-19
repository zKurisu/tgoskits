use alloc::collections::vec_deque::VecDeque;
use spin::Mutex;

use crate::platform_if::PlatformImpl;

use super::tcb::{TaskControlBlock, TaskState, current};

static IDLE: Mutex<VecDeque<TaskControlBlock>> = Mutex::new(VecDeque::new());
static FINISHED: Mutex<VecDeque<TaskControlBlock>> = Mutex::new(VecDeque::new());

pub fn schedule() {
    let idle = idle_pop();
    if let Some(mut idle) = idle {
        let mut cu = current();
        if matches!(cu.info().state, TaskState::Running) {
            cu.info_mut().state = TaskState::Suspend;
        }
        idle.info_mut().state = TaskState::Running;

        cu.switch_to(&idle);
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
    let mut g = IDLE.lock();
    while let Some(one) = g.pop_front() {
        if matches!(one.info().state, TaskState::Stopped) {
            unsafe { one.drop() };
            continue;
        }
        return Some(one);
    }
    None
}

pub fn finished_push(tcb: TaskControlBlock) {
    FINISHED.lock().push_back(tcb);
}

pub fn suspend() {
    let mut current = current();
    current.info_mut().state = TaskState::Suspend;
    schedule();
}
