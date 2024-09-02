use sparreal_std::Platform;

struct PlatformImpl;

impl Platform for PlatformImpl {
    fn wait_for_interrupt() {
        aarch64_cpu::asm::wfi();
    }
}

sparreal_std::set_impl!(PlatformImpl);
