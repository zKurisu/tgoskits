use crate::Platform;
use core::fmt::Write;

pub fn boot_debug_hex(mut w: impl Write, v: u64) {
    const HEX_BUF_SIZE: usize = 20; // 最大长度，包括前缀"0x"和数字
    let mut hex_buf: [char; HEX_BUF_SIZE] = ['0'; HEX_BUF_SIZE];
    let mut n = v;
    let _ = w.write_str("0x");

    if n == 0 {
        w.write_str("0");
        return;
    }
    let mut i = 0;
    while n > 0 {
        let digit = n & 0xf;
        let ch = if digit < 10 {
            (b'0' + digit as u8) as char
        } else {
            (b'a' + (digit - 10) as u8) as char
        };
        n >>= 4; // 右移四位
        hex_buf[i] = ch;
        i += 1;
    }
    let s = &hex_buf[..i];
    for ch in s.iter().rev() {
        let _ = w.write_char(*ch);
    }
}

#[cfg(test)]
mod test {
    extern crate std;
    use core::fmt;

    use super::*;

    #[test]
    fn test_hex_fmt() {
        struct TestWriter;
        impl fmt::Write for TestWriter {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                std::println!("{}", s);
                Ok(())
            }
        }

        boot_debug_hex(TestWriter {}, 0x12345678);
    }
}
