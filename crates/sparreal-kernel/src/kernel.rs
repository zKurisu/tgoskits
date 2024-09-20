use core::{fmt, panic::PanicInfo, ptr::NonNull};

use driver_interface::Register;
use log::*;

use crate::{
    driver::device_tree::set_dtb_addr,
    executor,
    logger::KLogger,
    mem::{self, *},
    platform::{self, app_main},
    stdout::{self, EarlyDebugWrite},
};

pub use crate::driver::manager::*;

pub unsafe fn init_log_and_memory(kconfig: &KernelConfig) {
    set_dtb_addr(kconfig.dtb_addr);
    let _ = log::set_logger(&KLogger);
    log::set_max_level(LevelFilter::Trace);
    stdout::set_stdout(EarlyDebugWrite {});
    info!("Logger initialized.");

    mem::init(kconfig);

    let version = env!("CARGO_PKG_VERSION");

    let _ = stdout::print(format_args!("Welcome to sparreal\nVersion: {version}\n",));
}

/// New kernel and initialize memory.
///
/// # Safety
///
/// 1. BSS section should be zeroed.
/// 2. If has MMU, it should be enabled.
/// 3. alloc can be used after this function.
pub unsafe fn run() -> ! {
    executor::block_on(async {
        driver_manager().init().await;
    });

    app_main();
    loop {
        platform::wait_for_interrupt();
    }
}

#[derive(Clone, Copy)]
pub struct MemoryRange {
    pub start: Phys<u8>,
    pub size: usize,
}

impl MemoryRange {
    pub const fn new() -> Self {
        Self {
            start: Phys::new(),
            size: 0,
        }
    }
}

#[derive(Clone)]
pub struct KernelConfig {
    pub va_offset: usize,
    pub reserved_memory: Option<MemoryRange>,
    pub main_memory: MemoryRange,
    pub main_memory_heap_offset: usize,
    pub hart_stack_size: usize,
    pub early_debug_reg: Option<MemoryRange>,
    pub stack_top: Phys<u8>,
    pub cpu_count: usize,
    pub dtb_addr: Option<NonNull<u8>>,
}

impl KernelConfig {
    pub const fn new() -> Self {
        Self {
            va_offset: 0,
            reserved_memory: None,
            hart_stack_size: BYTES_1M * 2,
            early_debug_reg: None,
            main_memory: MemoryRange::new(),
            main_memory_heap_offset: 0,
            stack_top: Phys::new(),
            cpu_count: 1,
            dtb_addr: None,
        }
    }
}
