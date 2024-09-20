use core::{fmt, panic::PanicInfo, ptr::NonNull};

use log::*;

use crate::{
    driver::manager::DriverManager,
    executor,
    logger::{self, KLogger},
    mem::{MemoryManager, Phys, BYTES_1M},
    module::ModuleBase,
    platform::app_main,
    stdout::{self, EarlyDebugWrite},
    time::Time,
    Platform,
};

pub unsafe fn init_log_and_memory(kconfig: &KernelConfig) {
    let _ = log::set_logger(&KLogger);
    log::set_max_level(LevelFilter::Trace);
    stdout::set_stdout(EarlyDebugWrite {});
    info!("Logger initialized.");
    

    
}

pub struct Kernel<P>
where
    P: Platform,
{
    module_base: ModuleBase<P>,
    driver: DriverManager<P>,
}

impl<P> Kernel<P>
where
    P: Platform,
{
    /// New kernel and initialize memory.
    ///
    /// # Safety
    ///
    /// 1. BSS section should be zeroed.
    /// 2. If has MMU, it should be enabled.
    /// 3. alloc can be used after this function.
    pub unsafe fn new(cfg: KernelConfig) -> Self {
        debug!("Initializing kernel...");
        let memory = MemoryManager::new();
        memory.init(&cfg);
        let module_base = ModuleBase {
            memory,
            time: Time::new(),
        };

        let driver = DriverManager::new(module_base.clone());
        Self {
            module_base,
            driver,
        }
    }

    /// Kernel entry point.
    ///
    /// # Safety
    ///
    pub unsafe fn run(&self) -> ! {
        let driver_manager = self.module_driver();

        executor::block_on(async move {
            driver_manager.init_stdout().await;
            self.print_welcome();
            driver_manager.init().await;
        });
        app_main();
        loop {
            P::wait_for_interrupt();
        }
    }

    pub fn module_driver(&self) -> DriverManager<P> {
        self.driver.clone()
    }

    pub fn module_memory(&self) -> MemoryManager<P> {
        self.module_base.memory.clone()
    }

    /// Global panic handler.
    pub fn panic_handler(&self, info: &PanicInfo) -> ! {
        error!("{info}");
        P::wait_for_interrupt();
        unreachable!()
    }

    pub fn module_time(&self) -> Time<P> {
        self.module_base.time.clone()
    }

    fn print_welcome(&self) {
        let version = env!("CARGO_PKG_VERSION");

        let _ = stdout::print(format_args!("Welcome to sparreal\nVersion: {version}\n",));
        let _ = stdout::print(format_args!("{}\n", self.module_base.memory));
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
        }
    }
}
