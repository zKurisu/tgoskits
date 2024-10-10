use core::{
    arch::{asm, global_asm},
    ptr::{addr_of, slice_from_raw_parts_mut, NonNull},
};

use aarch64_cpu::{asm::barrier, registers::*};
use flat_device_tree::Fdt;
use mem::*;
use sparreal_kernel::*;
use tock_registers::interfaces::ReadWriteable;

use crate::{
    arch::{
        debug::{debug_print, init_debug, mmu_add_offset},
        PlatformImpl,
    },
    consts::*,
};

use super::{
    debug::{debug_hex, debug_println},
    mmu, VA_OFFSET,
};

global_asm!(include_str!("boot.S"));
global_asm!(include_str!("vectors.S"));

extern "C" {
    fn _skernel();
    fn _ekernel();
    fn _stack_top();
}

static mut KCONFIG: KernelConfig = KernelConfig::new();
static mut DTB_ADDR: usize = 0;

#[no_mangle]
unsafe extern "C" fn __rust_main(dtb_addr: usize, va_offset: usize) -> ! {
    clear_bss();
    KCONFIG.hart_stack_size = HART_STACK_SIZE;
    print_info(dtb_addr, va_offset);
    KCONFIG.va_offset = va_offset;
    VA_OFFSET = va_offset;

    let mut kernel_end = Phys::from(_ekernel as *const u8);

    let new_dtb_addr = sparreal_kernel::driver::move_dtb(
        dtb_addr as _,
        NonNull::new_unchecked(kernel_end.as_usize() as _),
    );

    if let Some(addr) = new_dtb_addr {
        debug_print("DTB moved to ");
        DTB_ADDR = addr.as_ptr() as usize;
        debug_hex(DTB_ADDR as _);
        debug_print(", size: ");
        debug_hex(addr.len() as _);
        debug_println("\r\n");
        kernel_end = kernel_end + addr.len();
    }

    debug_print("Kernel @");
    debug_hex(_skernel as *const u8 as usize as _);
    debug_print("\r\n");

    let kernel_start = Phys::from(_skernel as *const u8).align_down(BYTES_1M * 2);
    let kernel_size = kernel_end - kernel_start;

    if let Err(msg) = config_memory_by_fdt(kernel_start, kernel_size) {
        debug_println(msg);

        KCONFIG.main_memory.start = kernel_start;
        KCONFIG.main_memory_heap_offset = kernel_size;
        KCONFIG.main_memory.size = KCONFIG.main_memory_heap_offset + BYTES_1M * 16;
    }

    let table = mmu::init_boot_table(&*addr_of!(KCONFIG));

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

    KCONFIG.stack_top = KCONFIG.main_memory.start + KCONFIG.main_memory.size;

    let stack_top = KCONFIG.stack_top.as_usize() + va_offset;

    debug_print("stack top: ");
    debug_hex(stack_top as _);
    debug_print("\r\n");

    debug_print("TCR_EL1:");
    debug_hex(TCR_EL1.get());
    debug_println("\r\n");

    // let ptr = 0x3082ffe8 as *const u8;
    // debug_hex(ptr.read_volatile() as _);

    debug_println("table set");

    mmu_add_offset(va_offset);
    if DTB_ADDR > 0 {
        DTB_ADDR += va_offset;
    }
    // Enable the MMU and turn on I-cache and D-cache
    SCTLR_EL1.modify(SCTLR_EL1::M::Enable + SCTLR_EL1::C::Cacheable + SCTLR_EL1::I::Cacheable);
    PlatformImpl::flush_tlb(None);
    barrier::isb(barrier::SY);

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
}

#[no_mangle]
unsafe extern "C" fn __rust_main_after_mmu() -> ! {
    debug_println("MMU enabled");
    
    KCONFIG.dtb_addr = NonNull::new(DTB_ADDR as _);
    crate::boot(KCONFIG.clone());
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
            KCONFIG.early_debug_reg = Some(MemoryRange {
                start: reg.reg.into(),
                size: reg.size,
            });
            init_debug(reg);
        }
    }
    // let reg = StdoutReg {
    //     reg: 0x2800D000 as _,
    //     size: 0x1000,
    // };
    // KCONFIG.early_debug_reg = Some(MemoryRange {
    //     start: reg.reg.into(),
    //     size: reg.size,
    // });
    // init_debug(reg);

    debug_print("dtb @");
    debug_hex(dtb_addr as _);
    debug_print(" va_offset: ");
    debug_hex(va_offset as _);
    debug_print("\r\n");
}

unsafe fn device_tree() -> Option<Fdt<'static>> {
    return Fdt::from_ptr(DTB_ADDR as _).ok();
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

// unsafe fn other_cpu() -> ! {
//     loop {
//         asm!("wfe")
//     }
// }

unsafe fn config_memory_by_fdt(
    kernel_start: Phys<u8>,
    kernel_size: usize,
) -> Result<(), &'static str> {
    let fdt = device_tree().ok_or("FDT not found")?;
    for resv in fdt.memory_reservations() {
        let addr = Phys::from(resv.address()).align_down(BYTES_1G);
        if addr == kernel_start {
            let range = MemoryRange {
                start: addr,
                size: resv.size().max(kernel_size + fdt.total_size()),
            };
            KCONFIG.reserved_memory = Some(range);
            debug_print("Reserving memory kernel @");
            debug_hex(addr.as_usize() as _);
            debug_print(" size: ");
            debug_hex(range.size as _);
            debug_print("\r\n");
            break;
        }
    }

    let memory = fdt.memory().map_err(|_e| "memory node not found")?;

    for region in memory.regions() {
        KCONFIG.main_memory.start = (region.starting_address as usize).into();
        KCONFIG.main_memory.size = region.size.unwrap_or_default();
        debug_print("memory @");
        debug_hex(KCONFIG.main_memory.start.as_usize() as _);
        debug_print(", size: ");
        debug_hex(region.size.unwrap_or_default() as _);
        debug_print(" Kernel start: ");
        debug_hex(kernel_start.as_usize() as _);

        if KCONFIG.main_memory.start.as_usize() <= kernel_start.as_usize()
            && kernel_start.as_usize()
                < KCONFIG.main_memory.start.as_usize() + KCONFIG.main_memory.size
        {
            KCONFIG.main_memory_heap_offset =
                kernel_start.as_usize() + kernel_size - KCONFIG.main_memory.start.as_usize();
            debug_print(", Kernel is in this memory, used: ");
            debug_hex(KCONFIG.main_memory_heap_offset as _);
            debug_println("\r\n");
            return Ok(());
        } else {
            debug_println(", Kernel is not in this memory");
        }
    }
    if KCONFIG.main_memory.size == 0 {
        Err("No memory region found")
    } else {
        Ok(())
    }
}
