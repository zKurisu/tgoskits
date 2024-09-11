use core::panic::PanicInfo;

use crate::kernel::kernel;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel().panic_handler(info)
}
