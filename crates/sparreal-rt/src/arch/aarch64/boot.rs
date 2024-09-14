use core::{
    arch::{asm, global_asm},
    ptr::{self, slice_from_raw_parts_mut, NonNull},
};

use aarch64_cpu::{asm::barrier, registers::*};
use log::debug;
use tock_registers::interfaces::ReadWriteable;

use crate::{arch::debug::{debug_fmt, debug_print, init_debug}, kernel};

use super::{debug::{debug_hex, debug_println}, mmu};

global_asm!(include_str!("boot.S"));
global_asm!(include_str!("vectors.S"));

extern "C" {
    fn _skernel();
    fn _stack_top();
}

#[no_mangle]
unsafe extern "C" fn __rust_main(dtb_addr: usize, va_offset: usize) -> ! {
    clear_bss();
    debug_print("bss cleared");
    print_info(dtb_addr, va_offset);

    let table = mmu::init_boot_table(va_offset, NonNull::new_unchecked(dtb_addr as *mut u8));

    debug_print("table initialized");

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
    debug_print("table set");

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
    if MPIDR_EL1.matches_all(
        MPIDR_EL1::Aff0.val(0)
            + MPIDR_EL1::Aff1.val(0)
            + MPIDR_EL1::Aff2.val(0)
            + MPIDR_EL1::Aff3.val(0),
    ) {
        kernel::boot()
    } else {
        loop {
            aarch64_cpu::asm::wfe();
        }
    }
}

unsafe fn clear_bss() {
    extern "C" {
        fn _sbss();
        fn _ebss();
    }
    let bss = &mut *slice_from_raw_parts_mut(_sbss as *mut u8, _ebss as usize - _sbss as usize);
    bss.fill(0);
}



fn print_info(dtb_addr: usize, va_offset: usize){
    debug_print("dtb @");
    debug_hex(dtb_addr as _);
    debug_print(" va_offset: ");
    debug_hex(va_offset as _);
    debug_print("\r\n");
}


#[no_mangle]
unsafe extern "C" fn __switch_to_el1() {
    debug_print("switch to EL1");
    SPSel.write(SPSel::SP::ELx);
    SP_EL0.set(0);
    let current_el = CurrentEL.read(CurrentEL::EL);
    if current_el >= 2 {
        if current_el == 3 {
            // Set EL2 to 64bit and enable the HVC instruction.
            SCR_EL3.write(
                SCR_EL3::NS::NonSecure + SCR_EL3::HCE::HvcEnabled + SCR_EL3::RW::NextELIsAarch64,
            );
            // Set the return address and exception level.
            SPSR_EL3.write(
                SPSR_EL3::M::EL1h
                    + SPSR_EL3::D::Masked
                    + SPSR_EL3::A::Masked
                    + SPSR_EL3::I::Masked
                    + SPSR_EL3::F::Masked,
            );
            asm!(
                "
            adr      x2, _start_boot
            msr elr_el3, x2
            "
            );
        }
        // Disable EL1 timer traps and the timer offset.
        CNTHCTL_EL2.modify(CNTHCTL_EL2::EL1PCEN::SET + CNTHCTL_EL2::EL1PCTEN::SET);
        CNTVOFF_EL2.set(0);
        // Set EL1 to 64bit.
        HCR_EL2.write(HCR_EL2::RW::EL1IsAarch64);
        // Set the return address and exception level.
        SPSR_EL2.write(
            SPSR_EL2::M::EL1h
                + SPSR_EL2::D::Masked
                + SPSR_EL2::A::Masked
                + SPSR_EL2::I::Masked
                + SPSR_EL2::F::Masked,
        );

        asm!(
            "
            mov     x8, sp
            msr     sp_el1, x8
            MOV      x0, x19
            adr      x2, _el1_entry
            msr      elr_el2, x2
            eret
            "
        );
    } else {
        asm!("bl _el1_entry")
    }
}
