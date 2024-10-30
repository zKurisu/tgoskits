use core::panic::PanicInfo;

use log::error;

use crate::platform;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("{info}");

    unsafe {
        platform::shutdown();
    };
    unreachable!()
}
