use core::fmt::{self, Arguments, Write};

use aarch64_cpu::registers::*;
use log::*;
use sparreal_kernel::{
    logger::{KernelLogger, StdoutWrite},
    mem::Phys,
    util::{self, boot::StdoutReg},
};

use super::{PlatformImpl, VA_OFFSET};

static mut OUT_REG: usize = 0;

pub unsafe fn mmu_add_offset(va_offset: usize) {
    OUT_REG += va_offset;
}

pub unsafe fn put_debug(char: u8) {
    // const BASE: usize = 0;
    // const BASE: usize = 0x2800D000;
    // const BASE: usize = 0xFE20_1000;
    if OUT_REG == 0 {
        return;
    }

    let base = if SCTLR_EL1.matches_any(SCTLR_EL1::M::SET) {
        OUT_REG + VA_OFFSET
    } else {
        OUT_REG
    };

    let state = (base + 0x18) as *mut u8;
    let put = (base) as *mut u8;
    while (state.read_volatile() & (0x20 as u8)) != 0 {}
    put.write_volatile(char);
}

struct DebugLogger;

pub struct DebugWriter;

impl fmt::Write for DebugWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.bytes() {
            unsafe { put_debug(c) }
        }
        Ok(())
    }
}

impl Log for DebugLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let _ = DebugWriter {}.write_fmt(format_args!("{} {}\r\n", record.level(), record.args(),));
    }

    fn flush(&self) {}
}

static LOGGER: DebugLogger = DebugLogger;

pub fn init_debug(stdout: StdoutReg) {
    unsafe { OUT_REG = stdout.reg as usize };
}

impl StdoutWrite for DebugWriter {
    fn write_char(&self, ch: char) {
        unsafe { put_debug(ch as _) };
    }
}

static KERNEL_LOGGER: KernelLogger<PlatformImpl> = KernelLogger::new();

pub fn init_log() {
    sparreal_kernel::logger::set_stdout(&DebugWriter);
    let _ = log::set_logger(&KERNEL_LOGGER).map(|()| log::set_max_level(LevelFilter::Trace));
}

pub fn debug_println(d: &str) {
    let _ = DebugWriter {}.write_str(d);
    let _ = DebugWriter {}.write_str("\r\n");
}
pub fn debug_print(d: &str) {
    let _ = DebugWriter {}.write_str(d);
}
pub fn debug_fmt(args: Arguments<'_>) {
    let _ = DebugWriter {}.write_fmt(args);
}

pub fn debug_hex(v: u64) {
    util::boot::boot_debug_hex(DebugWriter {}, v);
}
