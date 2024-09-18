use core::{fmt, panic::PanicInfo, ptr::NonNull};

use log::*;

use crate::{
    driver::manager::DriverManager,
    executor,
    mem::{MemoryManager, Phys},
    module::ModuleBase,
    platform::app_main,
    stdout::Stdout,
    time::Time,
    util::boot::k_boot_debug,
    Platform,
};

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
            stdout: Stdout::new(),
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

    pub fn print(&self, args: fmt::Arguments) {
        self.module_base.stdout.print(args);
    }

    fn print_welcome(&self) {
        let version = env!("CARGO_PKG_VERSION");

        let _ = self.print(format_args!("Welcome to sparreal\nVersion: {version}\n",));
        let _ = self.print(format_args!("{}\n", self.module_base.memory));
    }
}

#[derive(Clone)]
pub struct KernelConfig {
    pub reserved_memory_start: Option<Phys<u8>>,
    pub reserved_memory_size: usize,
    pub memory_start: Phys<u8>,
    pub memory_used: usize,
    pub memory_size: usize,
    pub stack_bottom: Phys<u8>,
    pub stack_size: usize,
}

impl KernelConfig {
    pub const fn new() -> Self {
        Self {
            reserved_memory_start: None,
            reserved_memory_size: 0,
            memory_start: Phys::new(),
            memory_size: 0,
            stack_bottom: Phys::new(),
            stack_size: 0,
            memory_used: 0,
        }
    }
}
