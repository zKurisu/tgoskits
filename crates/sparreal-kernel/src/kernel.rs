use core::{arch::asm, marker::PhantomData, panic::PanicInfo, ptr::NonNull};

use log::error;

use crate::{ platform::app_main, Platform};

pub struct Kernel<P>
where
    P: Platform,
{

    _mark: PhantomData<P>,
}

impl<P> Kernel<P>
where
    P: Platform,
{
    pub const fn new() -> Self {
        Self {
            _mark: PhantomData,
        }
    }

    /// Kernel entry point.
    ///
    /// # Safety
    ///
    /// 1. BSS section should be zeroed.
    /// 2. If has MMU, it should be enabled.
    pub unsafe fn run(&self, cfg: KernelConfig) -> ! {
        app_main();
        loop {
            P::wait_for_interrupt();
        }
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
    pub dtb_addr: NonNull<u8>,
}
