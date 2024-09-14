use core::{cell::UnsafeCell, ptr::NonNull};

use log::LevelFilter;
use sparreal_kernel::KernelConfig;

use crate::{arch::PlatformImpl, driver};

static KERNEL: KernelWarper = KernelWarper::new();

pub type Kernel = sparreal_kernel::Kernel<PlatformImpl>;

struct KernelWarper(UnsafeCell<Option<Kernel>>);

impl KernelWarper {
    const fn new() -> Self {
        Self(UnsafeCell::new(None))
    }
}

unsafe impl Send for KernelWarper {}
unsafe impl Sync for KernelWarper {}

pub fn kernel() -> &'static Kernel {
    unsafe { KERNEL.0.get().as_mut().unwrap().as_mut().unwrap() }
}

extern "C" {
    fn _stack_top();
}

/// 通用启动流程
pub(crate) unsafe fn boot() -> ! {
    let heap_lma = NonNull::new_unchecked(_stack_top as *mut u8);

    let cfg = KernelConfig {
        heap_start: heap_lma,
    };

    let k = Kernel::new(cfg);

    KERNEL.0.get().replace(Some(k));
    init_boot_log();
    kernel().module_driver().register_all(driver::registers());
    kernel().run()
}

pub fn init_boot_log() {
    let _ = log::set_logger(kernel()).map(|()| log::set_max_level(LevelFilter::Trace));
}
