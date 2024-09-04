use core::{
    arch::global_asm,
    ptr::{slice_from_raw_parts_mut, NonNull},
};

use aarch64_cpu::registers::TTBR0_EL1;
use page_table::aarch64::flush_tlb;
use sparreal_kernel::KernelConfig;

use crate::kernel::kernel;

global_asm!(include_str!("boot.S"));
global_asm!(include_str!("vectors.S"));

extern "C" {
    fn _skernel();
    fn _stack_top();
}

#[no_mangle]
unsafe extern "C" fn __rust_main(dtb_addr: usize, va_offset: usize) -> ! {
    clear_bss();

    let cfg = KernelConfig {
        dtb_addr,
        heap_lma: NonNull::new_unchecked(_stack_top as *mut u8),
        kernel_lma: NonNull::new_unchecked(_skernel as *mut u8),
        va_offset,
    };

    kernel().run(cfg);
}

unsafe fn clear_bss() {
    extern "C" {
        fn _sbss();
        fn _ebss();
    }
    let bss = &mut *slice_from_raw_parts_mut(_sbss as *mut u8, _ebss as usize - _sbss as usize);
    bss.fill(0);
}
