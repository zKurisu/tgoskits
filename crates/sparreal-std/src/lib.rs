#![no_std]

mod lang_items;
pub mod mem;
pub mod platform;

pub use platform::Platform;
use platform::{app_main, wait_for_interrupt};
pub use sparreal_macros::entry;

/// .
///
/// # Safety
///
/// .
pub unsafe fn kernel_run() -> ! {
    app_main();
    wait_for_interrupt();
    unreachable!();
}
