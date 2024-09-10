use core::{arch::asm, marker::PhantomData, panic::PanicInfo, ptr::NonNull};

use log::error;

use crate::{
    driver::manager::DriverManager, executor, mem::MemoryManager, platform::app_main, sync::RwLock,
    Platform,
};

type Module<T> = RwLock<Option<T>>;

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
            mem: RwLock::new(None),
            driver: RwLock::new(None),
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
        let driver_manager = self.driver_manager();

        executor::block_on(async move {
            driver_manager.init().await;
        });

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
        let m = self.memory_manager();
        let mut driver = self.driver.write();
        if driver.is_none() {
            driver.replace(DriverManager::new(m));
        }
    }

    pub fn driver_manager(&self) -> DriverManager<P> {
        self.driver
            .read()
            .as_ref()
            .expect("driver is not initialized")
            .clone()
    }

    pub fn memory_manager(&self) -> MemoryManager<P> {
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
}

pub unsafe fn enable_mmu_then() {}

pub struct KernelConfig {
    pub heap_start: NonNull<u8>,
}
