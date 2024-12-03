use core::{
    arch::{asm, global_asm},
    cell::UnsafeCell,
    ptr::{slice_from_raw_parts_mut, NonNull},
};

use aarch64_cpu::{asm::barrier, registers::*};
use fdt_parser::Fdt;
use kernel::{BootConfig, MemoryReservedRange};
use mem::*;
use page_table_generic::{AccessSetting, CacheSetting};
use platform::PlatformPageTable;
use sparreal_kernel::*;
use tock_registers::interfaces::ReadWriteable;

use crate::{
    arch::{
        debug::{init_debug, mmu_add_offset},
        mmu::PageTableImpl,
    },
    consts::*,
};

use super::mmu;

global_asm!(include_str!("boot.S"));
global_asm!(include_str!("vectors.S"));

extern "C" {
    fn _skernel();
    fn _ekernel();
    fn _stack_top();
}

pub(crate) static BOOT_INFO: BootInfoWapper = BootInfoWapper::new();

#[no_mangle]
unsafe extern "C" fn __rust_main(dtb_addr: usize, va_offset: usize) -> ! {
    clear_bss();
    print_info(dtb_addr, va_offset);
    BOOT_INFO.as_mut().va_offset = va_offset;

    let mut kernel_end = Phys::from(_ekernel as *const u8);

    let new_dtb_addr = sparreal_kernel::driver::move_dtb(
        dtb_addr as _,
        NonNull::new_unchecked(kernel_end.as_usize() as _),
    );

    if let Some(addr) = new_dtb_addr {
        BOOT_INFO.as_mut().fdt_addr = addr.as_ptr() as usize;

        dbg!("DTB moved to ");
        dbg_hex!(BOOT_INFO.as_ref().fdt_addr);
        dbg!(", size: ");
        dbg_hexln!(addr.len());
        kernel_end = kernel_end + addr.len();

        dbg!("DCache line size: ");
        dbg_hexln!(dcache_line_size());
    }

    dbg!("Kernel @");
    dbg_hexln!(_skernel as *const u8 as usize);

    let kernel_start = Phys::from(_skernel as *const u8).align_down(BYTES_1M * 2);
    let kernel_size = kernel_end - kernel_start;

    if let Err(msg) = config_memory_by_fdt(kernel_start, kernel_size) {
        dbgln!(msg);

        BOOT_INFO.as_mut().main_memory.start = kernel_start;
        BOOT_INFO.as_mut().main_memory_heap_offset = kernel_size;
        BOOT_INFO.as_mut().main_memory.size =
            BOOT_INFO.as_ref().main_memory_heap_offset + BYTES_1M * 16;
    }

    let table = mmu::init_boot_table();

    dbgln!("table initialized");

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

    // 需要先清缓存
    PageTableImpl::flush_tlb(None);

    // Set both TTBR0 and TTBR1
    TTBR1_EL1.set_baddr(table);
    TTBR0_EL1.set_baddr(table);

    BOOT_INFO.as_mut().stack_top =
        BOOT_INFO.as_ref().main_memory.start + BOOT_INFO.as_ref().main_memory.size;

    let stack_top = BOOT_INFO.as_ref().stack_top.as_usize() + va_offset;

    dbg!("stack top: ");
    dbg_hexln!(stack_top);
    dbg!("TCR_EL1:");
    dbg_hexln!(TCR_EL1.get());
    dbgln!("table set");

    mmu_add_offset(va_offset);
    if BOOT_INFO.as_ref().fdt_addr > 0 {
        BOOT_INFO.as_mut().fdt_addr += va_offset;
    }
    PageTableImpl::flush_tlb(None);
    // Enable the MMU and turn on I-cache and D-cache
    SCTLR_EL1.modify(SCTLR_EL1::M::Enable + SCTLR_EL1::C::Cacheable + SCTLR_EL1::I::Cacheable);
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
    dbgln!("MMU enabled");

    let cfg = KernelConfig {
        boot_info: BOOT_INFO.to_boot_config(),
        stack_top: BOOT_INFO.as_ref().stack_top,
        dtb_addr: NonNull::new(BOOT_INFO.as_ref().fdt_addr as _),
    };

    crate::boot(cfg);
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
            BOOT_INFO.as_mut().early_debug_reg = Some(MemoryRange {
                start: reg.reg.into(),
                size: reg.size,
            });
            init_debug(reg);
        }
    }
    if let Some(dtb) = NonNull::new(dtb_addr as *mut u8) {
        if let Ok(fdt) = fdt_parser::Fdt::from_ptr(dtb) {
            let cpu = fdt.boot_cpuid_phys();
            dbg!("DTB boot CPU: ");
            dbg_hexln!(cpu);
        }
    }

    dbg!("dtb @");
    dbg_hex!(dtb_addr);
    dbg!(" va_offset: ");
    dbg_hexln!(va_offset);
}

fn device_tree() -> Option<Fdt<'static>> {
    return Fdt::from_ptr(NonNull::new(BOOT_INFO.as_ref().fdt_addr as _)?).ok();
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

unsafe fn config_memory_by_fdt(
    kernel_start: Phys<u8>,
    kernel_size: usize,
) -> Result<(), &'static str> {
    let fdt = device_tree().ok_or("FDT not found")?;

    for node in fdt.memory() {
        for region in node.regions() {
            let address = region.address as usize;
            dbg!("memory @");
            dbg_hex!(address);
            dbg!(", size: ");
            dbg_hexln!(region.size);

            if address <= kernel_start.as_usize() && kernel_start.as_usize() < address + region.size
            {
                BOOT_INFO.as_mut().main_memory.start = address.into();
                BOOT_INFO.as_mut().main_memory.size = region.size;
                BOOT_INFO.as_mut().main_memory_heap_offset =
                    kernel_start.as_usize() + kernel_size - address;

                dbg!("Kernel start: ");
                dbg_hex!(kernel_start.as_usize());
                dbg!(", Kernel is in this memory, used: ");
                dbg_hexln!(BOOT_INFO.as_ref().main_memory_heap_offset);

                return Ok(());
            }
        }
    }
    if BOOT_INFO.as_mut().main_memory.size == 0 {
        Err("No memory region found")
    } else {
        Ok(())
    }
}

pub(crate) struct BootInfoWapper(UnsafeCell<BootInfo>);

unsafe impl Sync for BootInfoWapper {}

impl BootInfoWapper {
    pub const fn new() -> Self {
        Self(UnsafeCell::new(BootInfo::new()))
    }

    pub unsafe fn as_mut(&self) -> &mut BootInfo {
        &mut *self.0.get()
    }

    pub fn as_ref(&self) -> &BootInfo {
        unsafe { &*self.0.get() }
    }

    pub fn to_boot_config(&self) -> BootConfig {
        self.as_ref().to_boot_config()
    }
}

pub(crate) struct BootInfo {
    va_offset: usize,
    fdt_addr: usize,
    main_memory: MemoryRange,
    main_memory_heap_offset: usize,
    early_debug_reg: Option<MemoryRange>,
    stack_top: Phys<u8>,
}

unsafe impl Send for BootInfo {}

impl BootInfo {
    const fn new() -> Self {
        Self {
            va_offset: 0,
            fdt_addr: 0,
            main_memory: MemoryRange::new(),
            main_memory_heap_offset: 0,
            early_debug_reg: None,
            stack_top: Phys::new(),
        }
    }

    pub fn to_boot_config(&self) -> BootConfig {
        let mut reserved_memory = [None; 24];

        if let Some(reg) = self.early_debug_reg {
            reserved_memory[0] = Some(MemoryReservedRange {
                start: reg.start,
                size: reg.size,
                access: AccessSetting::Read | AccessSetting::Write,
                cache: CacheSetting::Device,
                name: "debug uart",
            });
        }
        BootConfig {
            main_memory: self.main_memory,
            main_memory_heap_offset: self.main_memory_heap_offset,
            hart_stack_size: HART_STACK_SIZE,
            reserved_memory,
            va_offset: self.va_offset,
        }
    }
}
