use core::{marker::PhantomData, panic::PanicInfo};

use log::error;

use crate::{platform::app_main, Platform};

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
        Self { _mark: PhantomData }
    }

    /// Kernel entry point.
    ///
    /// # Safety
    ///
    /// 1. BSS section should be zeroed.
    pub unsafe fn run(&self) -> ! {
        app_main();
        P::wait_for_interrupt();
        unreachable!()
    }

    /// Global panic handler.
    pub fn panic_handler(&self, info: &PanicInfo) -> ! {
        error!("{info}");
        P::wait_for_interrupt();
        unreachable!()
    }
}
