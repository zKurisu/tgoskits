use aarch64_cpu::{asm::*, registers::*};

use crate::arch::entry::el_entry;

pub fn switch_to_elx() {
    unsafe extern "C" {
        fn __cpu0_stack_top();
    }

    SPSel.write(SPSel::SP::ELx);
    SP_EL0.set(0);
    let current_el = CurrentEL.read(CurrentEL::EL);
    if current_el >= 2 {
        let el_entry = sym_addr!(el_entry);
        let sp = sym_addr!(__cpu0_stack_top);

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
            let switch = sym_addr!(switch_to_elx);

            ELR_EL3.set(switch as _);
            SP_EL2.set(sp as _);
            barrier::isb(barrier::SY);
            eret();
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

        ELR_EL2.set(el_entry as _);
        SP_EL1.set(sp as _);
        barrier::isb(barrier::SY);
        eret();
    }

    el_entry();
}
