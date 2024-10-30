use core::fmt::{self, Write};

use sparreal_kernel::util::{self, boot::StdoutReg};

static mut OUT_REG: usize = 0;

pub unsafe fn mmu_add_offset(va_offset: usize) {
    OUT_REG += va_offset;
}

pub unsafe fn put_debug(char: u8) {
    #[cfg(feature = "early_print")]
    {
        // const BASE: usize = 0;
        // const BASE: usize = 0x2800D000;
        // const BASE: usize = 0xFE20_1000;
        if OUT_REG == 0 {
            return;
        }

        // let base = if SCTLR_EL1.matches_any(SCTLR_EL1::M::SET) {
        //     OUT_REG + VA_OFFSET
        // } else {
        //     OUT_REG
        // };

        let base = OUT_REG;

        let state = (base + 0x18) as *mut u8;
        let put = (base) as *mut u8;
        while (state.read_volatile() & (0x20 as u8)) != 0 {}
        put.write_volatile(char);
    }
}

pub struct DebugWriter;

impl fmt::Write for DebugWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.bytes() {
            unsafe { put_debug(c) }
        }
        Ok(())
    }
}

pub fn init_debug(stdout: StdoutReg) {
    unsafe { OUT_REG = stdout.reg as usize };
}

pub fn debug_println(d: &str) {
    let _ = DebugWriter {}.write_str(d);
    let _ = DebugWriter {}.write_str("\r\n");
}
pub fn debug_print(d: &str) {
    let _ = DebugWriter {}.write_str(d);
}

pub fn debug_hex(v: u64) {
    util::boot::boot_debug_hex(DebugWriter {}, v);
}
