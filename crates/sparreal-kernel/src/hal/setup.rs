use kernutil::memory::MemoryDescriptor;

pub fn setup_allocator(regions: &[MemoryDescriptor]) {
    crate::os::logger::init();
    info!("Setting up allocator...");
    crate::os::mem::init_heap(regions);
}

pub fn setup() -> ! {
    unsafe extern "C" {
        fn __sparreal_main() -> !;
    }

    unsafe { __sparreal_main() }
}
