use core::panic::PanicInfo;

use log::error;

use crate::platform;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("{info}");

    loop {
        unsafe { platform::wait_for_interrupt() };
    }
}
