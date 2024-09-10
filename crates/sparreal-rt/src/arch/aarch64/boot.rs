use core::{
    arch::{asm, global_asm},
    ptr::{slice_from_raw_parts_mut, NonNull},
};

use aarch64_cpu::{asm::barrier, registers::*};
use sparreal_kernel::KernelConfig;
use tock_registers::interfaces::ReadWriteable;

use crate::kernel::kernel;

use super::{driver::register_drivers, mmu};

global_asm!(include_str!("boot.S"));
global_asm!(include_str!("vectors.S"));

extern "C" {
    fn _skernel();
    fn _stack_top();
}

#[no_mangle]
unsafe extern "C" fn __rust_main(dtb_addr: usize, va_offset: usize) -> ! {
    clear_bss();
    let table = mmu::init_boot_table(va_offset, NonNull::new_unchecked(dtb_addr as *mut u8));

    // Enable TTBR0 and TTBR1 walks, page size = 4K, vaddr size = 48 bits, paddr size = 40 bits.
    let tcr_flags0 = TCR_EL1::EPD0::EnableTTBR0Walks
        + TCR_EL1::TG0::KiB_4
        + TCR_EL1::SH0::Inner
        + TCR_EL1::ORGN0::WriteBack_ReadAlloc_WriteAlloc_Cacheable
        + TCR_EL1::IRGN0::WriteBack_ReadAlloc_WriteAlloc_Cacheable
        + TCR_EL1::T0SZ.val(16);
    let tcr_flags1 = TCR_EL1::EPD1::EnableTTBR1Walks
        + TCR_EL1::TG1::KiB_4
        + TCR_EL1::SH1::Inner
        + TCR_EL1::ORGN1::WriteBack_ReadAlloc_WriteAlloc_Cacheable
        + TCR_EL1::IRGN1::WriteBack_ReadAlloc_WriteAlloc_Cacheable
        + TCR_EL1::T1SZ.val(16);
    TCR_EL1.write(TCR_EL1::IPS::Bits_48 + tcr_flags0 + tcr_flags1);
    barrier::isb(barrier::SY);

    // Set both TTBR0 and TTBR1
    TTBR1_EL1.set_baddr(table);
    TTBR0_EL1.set_baddr(table);

    // Enable the MMU and turn on I-cache and D-cache
    SCTLR_EL1.modify(SCTLR_EL1::M::Enable + SCTLR_EL1::C::Cacheable + SCTLR_EL1::I::Cacheable);
    barrier::isb(barrier::SY);

    asm!("
    ADD  sp, sp, {offset}
    ADD  x30, x30, {offset}
    LDR      x8, =__rust_main_after_mmu
    BLR      x8
    B       .
    ", 
    offset = in(reg) va_offset,
    options(noreturn)
    )
}

#[no_mangle]
unsafe extern "C" fn __rust_main_after_mmu() -> ! {
    let heap_lma = NonNull::new_unchecked(_stack_top as *mut u8);

    let cfg = KernelConfig {
        heap_start: heap_lma,
    };
    kernel().preper_memory(&cfg);
    register_drivers();
    kernel().run(cfg)
}

unsafe fn clear_bss() {
    extern "C" {
        fn _sbss();
        fn _ebss();
    }
    let bss = &mut *slice_from_raw_parts_mut(_sbss as *mut u8, _ebss as usize - _sbss as usize);
    bss.fill(0);
}
