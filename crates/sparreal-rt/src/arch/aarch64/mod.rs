mod boot;
mod trap;

use sparreal_kernel::Platform;

pub struct PlatformImpl;

impl Platform for PlatformImpl {
    fn wait_for_interrupt() {
        aarch64_cpu::asm::wfi();
    }
}


