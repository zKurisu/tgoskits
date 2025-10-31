use core::ffi::c_void;

use acpi::{Handle, Handler};

static mut RSDP: usize = 0;

pub(crate) fn set_rsdp(addr: *const c_void) {
    unsafe {
        RSDP = addr as usize;
    }
}

fn rsdp() -> *const c_void {
    unsafe { RSDP as _ }
}

#[derive(Clone)]
struct AcpiHandle;
