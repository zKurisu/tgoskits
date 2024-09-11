use core::fmt::Write;

use alloc::boxed::Box;
use driver_interface::io;

use crate::sync::RwLock;

static STDOUT: RwLock<Option<Box<dyn io::Write>>> = RwLock::new(None);

pub(crate) fn set_stdout(stdout: Box<dyn io::Write>) {
    *STDOUT.write() = Some(stdout);
}

pub fn print(args: core::fmt::Arguments) {
    let mut stdout = STDOUT.write();
    if let Some(stdout) = stdout.as_mut() {
        let _ = stdout.write_fmt(args);
    }
}
