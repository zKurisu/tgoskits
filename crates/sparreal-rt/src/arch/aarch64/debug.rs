use sparreal_kernel::util::boot::StdoutReg;

static mut OUT_REG: usize = 0;

pub unsafe fn mmu_add_offset(va_offset: usize) {
    OUT_REG += va_offset;
}

#[cfg(feature = "early-print")]
pub unsafe fn put_debug(char: u8) {
    if OUT_REG == 0 {
        return;
    }
    let base = OUT_REG;

    let state = (base + 0x18) as *mut u8;
    let put = (base) as *mut u8;
    while (state.read_volatile() & 0x20_u8) != 0 {}
    put.write_volatile(char);
}

pub fn init_debug(stdout: StdoutReg) {
    unsafe { OUT_REG = stdout.reg as usize };
}
