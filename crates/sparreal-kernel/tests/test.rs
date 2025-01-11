use sparreal_kernel::util::boot::boot_debug_hex;
use std::fmt;

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
