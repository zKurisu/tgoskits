use core::cell::UnsafeCell;

use crate::arch::PlatformImpl;

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

pub fn kernel() -> &'static Kernel {
    unsafe { KERNEL.0.get().as_mut().unwrap() }
}
