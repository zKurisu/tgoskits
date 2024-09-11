use core::panic::PanicInfo;

use crate::kernel::get;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    get().panic_handler(info)
}
