use core::{arch::asm, marker::PhantomData, panic::PanicInfo, ptr::NonNull};

use log::error;

use crate::{mem::mmu::MMU, platform::app_main, Platform};

pub struct Kernel<P>
where
    P: Platform,
{
    pub mmu: MMU,
    _mark: PhantomData<P>,
}

impl<P> Kernel<P>
where
    P: Platform,
{
    pub const fn new() -> Self {
        Self {
            _mark: PhantomData,
            mmu: MMU::new(),
        }
    }

    /// Kernel entry point.
    ///
    /// # Safety
    ///
    /// 1. BSS section should be zeroed.
    pub unsafe fn run(&self, cfg: KernelConfig) -> ! {
        self.mmu.enable(&cfg);
        asm!(
            "
    LDR      x8, =__sparreal_rt_main
    BLR      x8
    B       .
        "
        );
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


    pub fn setup(&self){
        let a = 1;
        let b = 2;
    }
}

pub unsafe fn enable_mmu_then() {}

pub struct KernelConfig {
    pub dtb_addr: usize,
    pub heap_lma: NonNull<u8>,
    pub kernel_lma: NonNull<u8>,
    pub va_offset: usize,
}
