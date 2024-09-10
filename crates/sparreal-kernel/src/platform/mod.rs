use memory_addr::{PhysAddr, VirtAddr};
pub use page_table_interface::PageTableFn;

pub trait Platform: Sync + Send {
    fn wait_for_interrupt();
    fn set_kernel_page_table(table: Self::Page);

    #[cfg(feature = "mmu")]
    type Page: PageTableFn;
}

pub enum PageAttribute {
    Read,
    Write,
    Device,
    Execute,
    NonCache,
}

pub enum PageError {
    NoMemory,
    Other,
}

// #[macro_export]
// macro_rules! set_impl {
//     ($t: ty) => {
//         #[no_mangle]
//         unsafe fn _sparreal_0_0_wait_for_interrupt() {
//             <$t as $crate::Platform>::wait_for_interrupt()
//         }
//     };
// }

// #[inline(always)]
// pub fn wait_for_interrupt() {
//     extern "Rust" {
//         fn _sparreal_0_0_wait_for_interrupt();
//     }

//     #[allow(clippy::unit_arg)]
//     unsafe {
//         _sparreal_0_0_wait_for_interrupt()
//     }
// }

pub fn app_main() {
    extern "C" {
        fn __sparreal_rt_main();
    }

    unsafe { __sparreal_rt_main() }
}
