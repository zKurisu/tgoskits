pub trait Platform {
    fn wait_for_interrupt();
}

#[macro_export]
macro_rules! set_impl {
    ($t: ty) => {
        #[no_mangle]
        unsafe fn _sparreal_0_0_wait_for_interrupt() {
            <$t as $crate::Platform>::wait_for_interrupt()
        }
    };
}

#[inline(always)]
pub fn wait_for_interrupt() {
    extern "Rust" {
        fn _sparreal_0_0_wait_for_interrupt();
    }

    #[allow(clippy::unit_arg)]
    unsafe {
        _sparreal_0_0_wait_for_interrupt()
    }
}

pub fn app_main() {
    extern "C" {
        fn __sparreal_rt_main();
    }

    unsafe { __sparreal_rt_main() }
}
