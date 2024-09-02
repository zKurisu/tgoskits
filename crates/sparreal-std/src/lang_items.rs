use core::panic::PanicInfo;

use log::error;

use crate::platform::wait_for_interrupt;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("{info}");

    wait_for_interrupt();
    unreachable!()
}
