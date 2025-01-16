use core::fmt;

use alloc::boxed::Box;
use spin::Mutex;

use crate::{boot::debug, platform_if::PlatformImpl};

static STDOUT: Mutex<Option<Box<dyn fmt::Write + Send>>> = Mutex::new(None);

pub fn stdout_use_debug() {
    *STDOUT.lock() = Some(Box::new(debug::DebugWriter {}));
}

pub fn print(args: fmt::Arguments<'_>) {
    let mut g = STDOUT.lock();

    if let Some(ref mut writer) = *g {
        let _ = writer.write_fmt(args);
    }
}

pub fn early_dbg(s: &str) {
    for c in s.bytes() {
        let b = c as u8;
        PlatformImpl::debug_put(b);
    }
}

pub fn early_dbgln(s: &str) {
    early_dbg(s);
    early_dbg("\r\n");
}

pub fn early_dbg_hexln(v: u64) {
    early_dbg_hex(v);
    early_dbg("\r\n");
}
pub fn early_dbg_mem(name: &str, mem: &[u8]) {
    let range = mem.as_ptr_range();
    early_dbg_range(name, (range.start as usize)..(range.end as usize));
}
pub fn early_dbg_range(name: &str, range: core::ops::Range<usize>) {
    early_dbg(name);
    early_dbg(": [");
    early_dbg_hex(range.start as _);
    early_dbg(", ");
    early_dbg_hex(range.end as _);
    early_dbg(")\r\n");
}

pub fn early_dbg_hex(v: u64) {
    const HEX_BUF_SIZE: usize = 20; // 最大长度，包括前缀"0x"和数字
    let mut hex_buf: [u8; HEX_BUF_SIZE] = [b'0'; HEX_BUF_SIZE];
    let mut n = v;
    early_dbg("0x");

    if n == 0 {
        early_dbg("0");
        return;
    }
    let mut i = 0;
    while n > 0 {
        let digit = n & 0xf;
        let ch = if digit < 10 {
            b'0' + digit as u8
        } else {
            b'a' + (digit - 10) as u8
        };
        n >>= 4; // 右移四位
        hex_buf[i] = ch;
        i += 1;
    }
    let s = &hex_buf[..i];
    for ch in s.iter().rev() {
        PlatformImpl::debug_put(*ch);
    }
}
