use core::arch::global_asm;

use aarch64_cpu::registers::TTBR0_EL1;
use page_table::aarch64::flush_tlb;

use crate::{arch::mmu, consts::STACK_SIZE, kernel::kernel_run};

global_asm!(include_str!("boot.S"));
global_asm!(include_str!("vectors.S"));

#[no_mangle]
unsafe extern "C" fn __rust_main(dtb_addr: usize) -> ! {
    let a = *(dtb_addr as *const u8);
    let b = a + 1;

    TTBR0_EL1.set_baddr(0);
    flush_tlb(None);

    let c = *((dtb_addr + 0xffff_0000_0000_0000) as *const u8);
    assert_eq!(a, c);

    mmu::test();

    kernel_run()
}

