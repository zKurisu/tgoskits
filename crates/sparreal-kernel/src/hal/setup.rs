pub fn setup_allocator() {}

pub fn setup() -> ! {
    unsafe extern "C" {
        fn __sparreal_main() -> !;
    }

    unsafe { __sparreal_main() }
}
