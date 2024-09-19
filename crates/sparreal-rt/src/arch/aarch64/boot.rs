use core::{
    arch::{asm, global_asm},
    mem,
    ptr::{self, slice_from_raw_parts_mut, NonNull},
};

use aarch64_cpu::{asm::barrier, registers::*};
use flat_device_tree::Fdt;
use log::{debug, info};
use sparreal_kernel::{
    mem::{Addr, Phys},
    util, KernelConfig,
};
use tock_registers::interfaces::ReadWriteable;
use DAIF::A;

use crate::{
    arch::debug::{debug_fmt, debug_print, init_debug, mmu_add_offset},
    consts::{BYTES_1G, BYTES_1M, HART_STACK_SIZE, STACK_SIZE},
    kernel,
    mem::{MemoryMap, MemoryRange},
};

use super::{
    debug::{debug_hex, debug_println, init_log},
    mmu,
};

global_asm!(include_str!("boot.S"));
global_asm!(include_str!("vectors.S"));

extern "C" {
    fn _skernel();
    fn _ekernel();
    fn _stack_top();
}

static mut KCONFIG: KernelConfig = KernelConfig::new();

#[no_mangle]
unsafe extern "C" fn __rust_main(dtb_addr: usize, va_offset: usize) -> ! {
    clear_bss();
    KCONFIG.hart_stack_size = HART_STACK_SIZE;
    print_info(dtb_addr, va_offset);

    let kernel_start = Phys::<u8>::from(_skernel as *const u8 as usize).align_down(BYTES_1G);
    let kernel_end = Phys::<u8>::from(_ekernel as *const u8 as usize);
    let mut kernel_size = kernel_end.as_usize() - kernel_start.as_usize();

    unsafe fn no_memory() {
        let kernel_start = Phys::<u8>::from(_skernel as *const u8 as usize).align_down(BYTES_1G);
        let kernel_end = Phys::<u8>::from(_ekernel as *const u8 as usize);
        KCONFIG.memory_start = kernel_start;
        KCONFIG.memory_heap_start = kernel_end.as_usize() - kernel_start.as_usize();
        KCONFIG.memory_size = KCONFIG.memory_heap_start + BYTES_1M * 16;
        debug_println("FDT parse failed!");
    }

    match Fdt::from_ptr(dtb_addr as _) {
        Ok(fdt) => {
            if let Ok(memory) = fdt.memory() {
                if let Some(region) = memory.regions().next() {
                    KCONFIG.memory_start = (region.starting_address as usize).into();
                    KCONFIG.memory_size = region.size.unwrap_or_default();
                    debug_print("memory region: 0");

                    if KCONFIG.memory_start == kernel_start {
                        KCONFIG.memory_start = kernel_start;
                        KCONFIG.memory_heap_start =
                            kernel_end.as_usize() - kernel_start.as_usize() + fdt.total_size();
                        debug_print(", Image is this memory, used: ");
                        debug_hex(KCONFIG.memory_heap_start as _);
                        debug_println("\r\n");
                    } else {
                        debug_println(", Image is not in this memory");
                    }
                } else {
                    no_memory();
                }
            } else {
                no_memory();
            }

            for resv in fdt.memory_reservations() {
                let addr = Phys::<u8>::from(resv.address() as usize).align_down(BYTES_1G);
                if addr == kernel_start {
                    KCONFIG.reserved_memory_start = Some(addr);
                    KCONFIG.reserved_memory_size = resv.size().max(kernel_size + fdt.total_size());
                    debug_print("Reserving memory kernel @");
                    debug_hex(addr.as_usize() as _);
                    debug_print("\r\n");
                    break;
                }
            }
        }
        Err(_) => no_memory(),
    }

    let table = mmu::init_boot_table(va_offset, NonNull::new_unchecked(dtb_addr as *mut u8));

    debug_println("table initialized");

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
    debug_println("table set");
    mmu_add_offset(va_offset);
    // Enable the MMU and turn on I-cache and D-cache
    SCTLR_EL1.modify(SCTLR_EL1::M::Enable + SCTLR_EL1::C::Cacheable + SCTLR_EL1::I::Cacheable);
    barrier::isb(barrier::SY);

    let stack_top = (KCONFIG.memory_start + KCONFIG.memory_size).as_usize() + va_offset;
    debug_print("stack top: ");
    debug_hex(stack_top as _);
    debug_print("\r\n");

    asm!("
    MOV  sp,  {sp_top}
    ADD  x30, x30, {offset}
    LDR      x8, =__rust_main_after_mmu
    BLR      x8
    B       .
    ", 
    sp_top = in(reg) stack_top,
    offset = in(reg) va_offset,
    options(noreturn)
    )
    // asm!("
    // ADD  sp, sp, {offset}
    // ADD  x30, x30, {offset}
    // LDR      x8, =__rust_main_after_mmu
    // BLR      x8
    // B       .
    // ",
    // offset = in(reg) va_offset,
    // options(noreturn)
    // )
}

#[no_mangle]
unsafe extern "C" fn __rust_main_after_mmu() -> ! {
    debug_println("MMU ok");
    init_log();
    debug!("Debug logger ok");
    debug!(
        "CPU: {:?}.{:?}.{:?}.{:?}",
        MPIDR_EL1.read(MPIDR_EL1::Aff0),
        MPIDR_EL1.read(MPIDR_EL1::Aff1),
        MPIDR_EL1.read(MPIDR_EL1::Aff2),
        MPIDR_EL1.read(MPIDR_EL1::Aff3)
    );

    if MPIDR_EL1.matches_all(MPIDR_EL1::Aff0.val(0)) {
        info!("Kernel start");
        kernel::boot(KCONFIG.clone())
    } else {
        info!("wait for primary cpu");
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

unsafe fn print_info(dtb_addr: usize, va_offset: usize) {
    if let Some(dtb) = NonNull::new(dtb_addr as *mut u8) {
        if let Some(reg) = util::boot::stdout_reg(dtb) {
            KCONFIG.debug_reg_start = Some(Phys::from(reg.reg));
            KCONFIG.debug_reg_size = reg.size;
            init_debug(reg);
        }
    }

    debug_print("dtb @");
    debug_hex(dtb_addr as _);
    debug_print(" va_offset: ");
    debug_hex(va_offset as _);
    debug_print("\r\n");
}

#[no_mangle]
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

unsafe fn other_cpu() -> ! {
    loop {
        asm!("wfe")
    }
}
