use core::{
    arch::{asm, global_asm},
    ptr::NonNull,
};

use crate::mem::clear_bss;
use aarch64_cpu::registers::*;
use sparreal_kernel::{io::print::*, platform::PlatformInfoKind};

global_asm!(include_str!("boot.s"));

#[unsafe(no_mangle)]
extern "C" fn __rust_boot(va_offset: usize, fdt_addr: usize) {
    unsafe {
        clear_bss();

        let platform_info: PlatformInfoKind = if let Some(addr) = NonNull::new(fdt_addr as _) {
            PlatformInfoKind::new_fdt(addr)
        } else {
            todo!()
        };

        if let Some(info) = platform_info.debugcon() {
            crate::debug::init_by_info(info);
        }

        let rsv = [];

        if let Err(s) = sparreal_kernel::boot::start(va_offset, platform_info, &rsv) {
            early_dbgln(s);
        }
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn __switch_to_el1() {
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
            unsafe {
                asm!(
                    "
            adr      x2, _start_boot
            msr elr_el3, x2
            "
                );
            }
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
        unsafe {
            asm!(
                "
            mov     x8, sp
            msr     sp_el1, x8
            MOV      x0, x19
            adr      x2, _el1_entry
            msr      elr_el2, x2
            eret
            "
            )
        };
    } else {
        unsafe { asm!("bl _el1_entry") }
    }
}
