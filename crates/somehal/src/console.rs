use core::fmt::Write;

pub fn _print(args: core::fmt::Arguments) {
    let _ = PrintFmt {}.write_fmt(args);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::console::_print(core::format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\r\n"));
    ($($arg:tt)*) => ($crate::console::_print(core::format_args!("{}{}", core::format_args!($($arg)*), "\r\n")));
}

struct PrintFmt {}

impl Write for PrintFmt {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        printer().write_str(s);
        Ok(())
    }
}

fn printer() -> &'static dyn Printer {
    unsafe { PRINT }
}

pub(crate) trait Printer: Send + Sync {
    fn write_str(&self, s: &str);
    fn read_byte(&self) -> Option<u8>;
}

struct NoPrinter;
impl Printer for NoPrinter {
    fn write_str(&self, _s: &str) {
        // Do nothing
    }

    fn read_byte(&self) -> Option<u8> {
        None
    }
}

static mut PRINT: &dyn Printer = &NoPrinter;

pub(crate) unsafe fn set_printer(printer: &'static dyn Printer) {
    unsafe {
        PRINT = printer;
    }
}
