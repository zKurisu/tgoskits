use kernutil::memory::MemoryDescriptor;

pub fn setup_allocator(free: &[MemoryDescriptor]) {
    crate::os::logger::init();
    info!("Setting up allocator...");
    crate::os::mem::init_heap(free);
}

pub fn setup() -> ! {
    unsafe extern "C" {
        fn __sparreal_main() -> !;
    }

    unsafe { __sparreal_main() }
}
