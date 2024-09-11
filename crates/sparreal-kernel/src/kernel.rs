use core::{arch::asm, fmt, marker::PhantomData, panic::PanicInfo, ptr::NonNull, time::Duration};

use alloc::vec::Vec;
use driver_interface::Register;
use log::{error, info};

use crate::{
    driver::manager::DriverManager, executor,  mem::MemoryManager, module::ModuleBase, platform::app_main, stdout::Stdout, sync::RwLock, time::Time, Platform
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
        let mut memory = MemoryManager::new();
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
    /// run after [`preper`]
    pub unsafe fn run(&self) -> ! {
        let driver_manager = self.module_driver();

        executor::block_on(async move {
            driver_manager.init().await;
        });
        info!("Welcome to sparreal!");
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

    pub fn print(&self, args: fmt::Arguments){
        self.module_base.stdout.print(args);
    }
}

pub struct KernelConfig {
    pub heap_start: NonNull<u8>,
}
