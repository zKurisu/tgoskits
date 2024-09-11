use core::{arch::asm, marker::PhantomData, panic::PanicInfo, ptr::NonNull, time::Duration};

use log::{error, info};

use crate::{
    driver::manager::DriverManager, executor, logger::init_boot_log, mem::MemoryManager,
    module::Module, platform::app_main, sync::RwLock, time::Time, Platform,
};

pub struct Kernel<P>
where
    P: Platform,
{
    mem: Module<MemoryManager<P>>,
    driver: Module<DriverManager<P>>,
}

impl<P> Kernel<P>
where
    P: Platform,
{
    pub const fn new() -> Self {
        Self {
            mem: Module::uninit(),
            driver: Module::uninit(),
        }
    }
    /// Kernel entry point.
    ///
    /// # Safety
    ///
    /// 1. BSS section should be zeroed.
    /// 2. If has MMU, it should be enabled.
    pub unsafe fn preper(&self, cfg: &KernelConfig) {
        self.new_memory_manager(cfg);
        self.new_driver_manager();
    }
    /// Kernel entry point.
    ///
    /// # Safety
    ///
    /// run after [`preper`]
    pub unsafe fn run(&self, cfg: KernelConfig) -> ! {
        let driver_manager = self.module_driver();

        executor::block_on(async move {
            driver_manager.init().await;
        });
        init_boot_log();
        info!("Welcome to sparreal!");
        app_main();
        loop {
            P::wait_for_interrupt();
        }
    }
    fn new_memory_manager(&self, cfg: &KernelConfig) {
        let mut m = self.mem.write();
        if m.is_none() {
            let mut manager = MemoryManager::new();
            unsafe {
                manager.init(cfg);
            }
            m.replace(manager);
        }
    }
    fn new_driver_manager(&self) {
        let m = self.module_memory();
        let mut driver = self.driver.write();
        if driver.is_none() {
            driver.replace(DriverManager::new(m));
        }
    }

    pub fn module_driver(&self) -> DriverManager<P> {
        self.driver
            .read()
            .as_ref()
            .expect("driver is not initialized")
            .clone()
    }

    pub fn module_memory(&self) -> MemoryManager<P> {
        self.mem
            .read()
            .as_ref()
            .expect("memory is not initialized")
            .clone()
    }

    /// Global panic handler.
    pub fn panic_handler(&self, info: &PanicInfo) -> ! {
        error!("{info}");
        P::wait_for_interrupt();
        unreachable!()
    }

    pub fn module_time(&self) -> Time<P> {
        Time::new()
    }
}

pub struct KernelConfig {
    pub heap_start: NonNull<u8>,
}
