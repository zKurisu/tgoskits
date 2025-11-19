use acpi::AcpiTables;
use core::ffi::c_void;

pub(crate) mod earlycon;
mod handle;

use crate::mem::phys_to_virt;
pub(crate) use handle::AcpiHandle;

/// RSDP存储
#[unsafe(link_section = ".data")]
static mut RSDP: usize = 0;

/// 设置RSDP地址
pub(crate) fn set_rsdp(addr: *const c_void) {
    unsafe {
        RSDP = addr as usize;
    }
}

/// 获取RSDP地址
fn rsdp() -> *const c_void {
    phys_to_virt(unsafe { RSDP as _ }) as *const c_void
}

pub fn tables() -> Result<AcpiTables<AcpiHandle>, acpi::AcpiError> {
    let h = AcpiHandle;
    unsafe { ::acpi::AcpiTables::from_rsdp(h, rsdp() as usize) }
}
