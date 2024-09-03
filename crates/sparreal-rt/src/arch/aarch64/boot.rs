use core::arch::global_asm;

use crate::{consts::STACK_SIZE, kernel::kernel_run};

global_asm!(include_str!("boot.S"));
global_asm!(include_str!("vectors.S"));

#[no_mangle]
unsafe extern "C" fn __rust_main(dtb_addr: usize) -> ! {
    kernel_run()
}
