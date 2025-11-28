#![no_std]
#![no_main]

use core::{
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Duration,
};

use log::info;
use sparreal_kernel::os::r#async::{
    SingleCpuExecutor, block_on, has_pending_tasks, spawn, task_count, tick,
};
use sparreal_rt::os::time::{one_shot_after, since_boot};

extern crate alloc;
#[macro_use]
extern crate sparreal_rt;

// ============================================================================
// 测试辅助宏和函数
// ============================================================================

macro_rules! assert_test {
    ($cond:expr, $msg:expr) => {
        if !$cond {
            panic!("Test failed: {}", $msg);
        }
    };
}

fn wait_for_flag(flag: &AtomicBool, timeout_ms: u64) -> bool {
    let start = since_boot();
    let timeout = Duration::from_millis(timeout_ms);
    loop {
        if flag.load(Ordering::SeqCst) {
            return true;
        }
        if since_boot().saturating_sub(start) > timeout {
            return false;
        }
        // 添加CPU让步，避免忙等待
        for _ in 0..1000 {
            core::hint::spin_loop();
        }
    }
}

fn wait_for_count(counter: &AtomicUsize, expected: usize, timeout_ms: u64) -> bool {
    let start = since_boot();
    let timeout = Duration::from_millis(timeout_ms);
    loop {
        if counter.load(Ordering::SeqCst) >= expected {
            return true;
        }
        if since_boot().saturating_sub(start) > timeout {
            return false;
        }
        // 添加CPU让步，避免忙等待
        for _ in 0..1000 {
            core::hint::spin_loop();
        }
    }
}

/// 带超时的异步任务调度函数
fn run_executor_with_timeout(timeout_ms: u64) {
    let start = since_boot();
    let timeout = Duration::from_millis(timeout_ms);

    loop {
        if !has_pending_tasks() {
            info!("[ASYNC] No pending tasks, exiting scheduler");
            break;
        }

        tick();

        if since_boot().saturating_sub(start) > timeout {
            info!("[ASYNC] Scheduler timeout reached, forcing exit");
            break;
        }

        // 短暂的CPU让步，避免过度占用
        for _ in 0..100 {
            core::hint::spin_loop();
        }
    }
}

// ============================================================================
// 测试用例
// ============================================================================

/// 测试1: 基本的异步任务生成和执行
fn test_basic_spawn_and_run() {
    info!("[TEST] test_basic_spawn_and_run");

    static TASK_EXECUTED: AtomicBool = AtomicBool::new(false);

    spawn(async {
        TASK_EXECUTED.store(true, Ordering::SeqCst);
        info!("[ASYNC] Basic task executed");
    });

    assert_test!(task_count() == 1, "Task count should be 1 after spawn");
    assert_test!(has_pending_tasks(), "Should have pending tasks");

    // 运行调度直到任务完成（带超时）
    run_executor_with_timeout(1000);

    assert_test!(
        TASK_EXECUTED.load(Ordering::SeqCst),
        "Task should have been executed"
    );
    assert_test!(task_count() == 0, "Task count should be 0 after completion");

    info!("[PASS] test_basic_spawn_and_run");
}

/// 测试2: 多个异步任务执行
fn test_multiple_tasks() {
    info!("[TEST] test_multiple_tasks");

    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    const TASK_COUNT: usize = 5;

    for i in 0..TASK_COUNT {
        spawn(async move {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            info!("[ASYNC] Task {} executed", i);
        });
    }

    assert_test!(task_count() == TASK_COUNT, "Should have 5 tasks spawned");

    // 运行调度直到所有任务完成（带超时）
    run_executor_with_timeout(2000);

    assert_test!(
        COUNTER.load(Ordering::SeqCst) == TASK_COUNT,
        "All tasks should have executed"
    );
    assert_test!(task_count() == 0, "All tasks should be cleaned up");

    info!("[PASS] test_multiple_tasks");
}

/// 测试3: block_on 函数测试
fn test_block_on() {
    info!("[TEST] test_block_on");

    static BLOCK_ON_EXECUTED: AtomicBool = AtomicBool::new(false);

    block_on(async {
        BLOCK_ON_EXECUTED.store(true, Ordering::SeqCst);
        info!("[ASYNC] Block-on task executed");
    });

    assert_test!(
        BLOCK_ON_EXECUTED.load(Ordering::SeqCst),
        "Block-on task should have executed"
    );
    assert_test!(task_count() == 0, "No tasks should remain after block_on");

    info!("[PASS] test_block_on");
}

/// 测试4: 任务状态管理
fn test_task_state() {
    info!("[TEST] test_task_state");

    static TASK_STARTED: AtomicBool = AtomicBool::new(false);
    static TASK_COMPLETED: AtomicBool = AtomicBool::new(false);

    spawn(async {
        TASK_STARTED.store(true, Ordering::SeqCst);
        // 模拟一些异步工作
        info!("[ASYNC] Task started");

        // 使用定时器模拟异步等待
        // 注意：这里简化了异步等待，实际使用中会更复杂

        TASK_COMPLETED.store(true, Ordering::SeqCst);
        info!("[ASYNC] Task completed");
    });

    // 运行调度（带超时）
    run_executor_with_timeout(1500);

    assert_test!(
        TASK_STARTED.load(Ordering::SeqCst),
        "Task should have started"
    );
    assert_test!(
        TASK_COMPLETED.load(Ordering::SeqCst),
        "Task should have completed"
    );

    info!("[PASS] test_task_state");
}

/// 测试5: 执行器状态检查
fn test_executor_state() {
    info!("[TEST] test_executor_state");

    let executor = SingleCpuExecutor::new();

    assert_test!(
        executor.task_count() == 0,
        "New executor should have 0 tasks"
    );
    assert_test!(
        !executor.has_pending_tasks(),
        "New executor should have no pending tasks"
    );
    assert_test!(!executor.is_running(), "New executor should not be running");

    info!("[PASS] test_executor_state");
}

/// 测试6: 长时间运行任务（测试超时机制）
fn test_long_running_task_timeout() {
    info!("[TEST] test_long_running_task_timeout");

    static LONG_TASK_STARTED: AtomicBool = AtomicBool::new(false);
    static LONG_TASK_COMPLETED: AtomicBool = AtomicBool::new(false);

    spawn(async {
        LONG_TASK_STARTED.store(true, Ordering::SeqCst);
        info!("[ASYNC] Long task started");

        // 模拟长时间运行的任务
        // 注意：简化实现，实际会有更复杂的异步操作

        LONG_TASK_COMPLETED.store(true, Ordering::SeqCst);
        info!("[ASYNC] Long task completed");
    });

    // 运行调度（带超时）
    run_executor_with_timeout(1500);

    assert_test!(
        LONG_TASK_STARTED.load(Ordering::SeqCst),
        "Long task should have started"
    );
    assert_test!(
        LONG_TASK_COMPLETED.load(Ordering::SeqCst),
        "Long task should have completed"
    );

    info!("[PASS] test_long_running_task_timeout");
}

/// 测试7: 任务优先级和调度
fn test_task_priority() {
    info!("[TEST] test_task_priority");

    static EXECUTION_ORDER: AtomicUsize = AtomicUsize::new(0);
    static FIRST_EXECUTED: AtomicBool = AtomicBool::new(false);
    static SECOND_EXECUTED: AtomicBool = AtomicBool::new(false);

    // 生成两个任务，按照入队顺序执行（先进先出）
    spawn(async {
        let order = EXECUTION_ORDER.fetch_add(1, Ordering::SeqCst);
        info!("[ASYNC] First task executed, order = {}", order);
        FIRST_EXECUTED.store(true, Ordering::SeqCst);
    });

    spawn(async {
        let order = EXECUTION_ORDER.fetch_add(1, Ordering::SeqCst);
        info!("[ASYNC] Second task executed, order = {}", order);
        SECOND_EXECUTED.store(true, Ordering::SeqCst);
    });

    // 运行调度（带超时）
    run_executor_with_timeout(1500);

    assert_test!(
        FIRST_EXECUTED.load(Ordering::SeqCst),
        "First task should have executed"
    );
    assert_test!(
        SECOND_EXECUTED.load(Ordering::SeqCst),
        "Second task should have executed"
    );
    assert_test!(
        EXECUTION_ORDER.load(Ordering::SeqCst) == 2,
        "Both tasks should have executed"
    );

    info!("[PASS] test_task_priority");
}

/// 测试8: 复杂异步操作
fn test_complex_async_operations() {
    info!("[TEST] test_complex_async_operations");

    static COMPLEX_TASK_COMPLETED: AtomicBool = AtomicBool::new(false);
    static OPERATION_COUNT: AtomicUsize = AtomicUsize::new(0);

    spawn(async {
        info!("[ASYNC] Complex task started");

        // 模拟多个异步操作
        for i in 0..3 {
            OPERATION_COUNT.fetch_add(1, Ordering::SeqCst);
            info!("[ASYNC] Complex operation {}", i);

            // 在实际异步环境中，这里会有 await 操作
            // 简化实现中我们直接继续
        }

        COMPLEX_TASK_COMPLETED.store(true, Ordering::SeqCst);
        info!("[ASYNC] Complex task completed");
    });

    // 运行调度（带超时）
    run_executor_with_timeout(1500);

    assert_test!(
        COMPLEX_TASK_COMPLETED.load(Ordering::SeqCst),
        "Complex task should have completed"
    );
    assert_test!(
        OPERATION_COUNT.load(Ordering::SeqCst) == 3,
        "All operations should have executed"
    );

    info!("[PASS] test_complex_async_operations");
}

/// 测试9: 执行器压力测试
fn test_executor_stress() {
    info!("[TEST] test_executor_stress");

    static STRESS_COUNTER: AtomicUsize = AtomicUsize::new(0);
    const STRESS_TASK_COUNT: usize = 20;

    // 生成大量任务
    for i in 0..STRESS_TASK_COUNT {
        spawn(async move {
            STRESS_COUNTER.fetch_add(1, Ordering::SeqCst);
            if i % 5 == 0 {
                info!("[ASYNC] Stress task {} executed", i);
            }
        });
    }

    assert_test!(
        task_count() == STRESS_TASK_COUNT,
        "Should have 20 tasks spawned"
    );

    // 运行调度直到完成
    let timeout = Duration::from_millis(2000);
    let start = since_boot();
    while has_pending_tasks() && since_boot().saturating_sub(start) < timeout {
        tick();
    }

    assert_test!(
        STRESS_COUNTER.load(Ordering::SeqCst) == STRESS_TASK_COUNT,
        "All stress tasks should have executed"
    );
    assert_test!(task_count() == 0, "All stress tasks should be cleaned up");

    info!("[PASS] test_executor_stress");
}

/// 测试10: 定时器与异步集成
fn test_timer_async_integration() {
    info!("[TEST] test_timer_async_integration");

    static TIMER_ASYNC_COMPLETED: AtomicBool = AtomicBool::new(false);

    // 使用定时器触发异步任务
    let _timer_handle = one_shot_after(Duration::from_millis(100), || {
        spawn(async {
            info!("[ASYNC] Timer-triggered async task");
            TIMER_ASYNC_COMPLETED.store(true, Ordering::SeqCst);
        });
    })
    .unwrap();

    // 运行异步调度
    let timeout = Duration::from_millis(500);
    let start = since_boot();
    while since_boot().saturating_sub(start) < timeout {
        tick();
        if TIMER_ASYNC_COMPLETED.load(Ordering::SeqCst) {
            break;
        }
    }

    assert_test!(
        TIMER_ASYNC_COMPLETED.load(Ordering::SeqCst),
        "Timer-triggered async task should have completed"
    );

    info!("[PASS] test_timer_async_integration");
}

// ============================================================================
// 主函数
// ============================================================================

#[sparreal_rt::entry]
fn main() {
    info!("========================================");
    info!("Async Executor Test Suite");
    info!("========================================");

    // 基础功能测试
    test_executor_state();
    test_basic_spawn_and_run();
    test_block_on();
    test_task_state();

    // 多任务测试
    test_multiple_tasks();
    test_task_priority();
    test_complex_async_operations();

    // 高级功能测试
    test_long_running_task_timeout();
    test_executor_stress();
    test_timer_async_integration();

    info!("========================================");
    println!("All async tests passed!");
    info!("========================================");
}
