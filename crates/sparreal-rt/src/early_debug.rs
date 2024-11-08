use core::{
    fmt::{self, Write},
    marker::PhantomData,
};

use sparreal_kernel::{util, Platform};

use crate::arch::PlatformImpl;

#[macro_export]
macro_rules! debug_hex {
    ($v:expr) => {
        $crate::early_debug::_debug_hex($v as _)
    };
}

pub(crate) fn _debug_hex(v: u64) {
    util::boot::boot_debug_hex(DebugWriter {}, v);
}

struct DebugWriter;

impl fmt::Write for DebugWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.bytes() {
            unsafe { PlatformImpl::debug_write_char(c) }
        }
        Ok(())
    }
}

pub(crate) fn debug_endl() {
    let _ = DebugWriter {}.write_str("\r\n");
}
pub(crate) fn debug_println(d: &str) {
    let _ = DebugWriter {}.write_str(d);
    debug_endl();
}
pub(crate) fn debug_print(d: &str) {
    let _ = DebugWriter {}.write_str(d);
}

pub(crate) fn debug_fmt(args: fmt::Arguments<'_>) {
    let _ = DebugWriter {}.write_fmt(args);
}
