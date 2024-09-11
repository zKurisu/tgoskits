use core::{cell::UnsafeCell, ptr::NonNull};

use sparreal_kernel::KernelConfig;

use crate::{arch::PlatformImpl, driver};

static KERNEL: KernelWarper = KernelWarper::new();

pub type Kernel = sparreal_kernel::Kernel<PlatformImpl>;

struct KernelWarper(UnsafeCell<Kernel>);

impl KernelWarper {
    const fn new() -> Self {
        Self(UnsafeCell::new(Kernel::new()))
    }
}

unsafe impl Send for KernelWarper {}
unsafe impl Sync for KernelWarper {}

pub fn get() -> &'static Kernel {
    unsafe { KERNEL.0.get().as_mut().unwrap() }
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
    get().preper(&cfg);
    driver::register_all();
    get().run(cfg);
    unreachable!()
}