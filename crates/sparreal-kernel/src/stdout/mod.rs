use core::fmt::{self, Write};

use alloc::sync::Arc;
use driver_interface::io;

use crate::{logger::StdoutWrite, sync::RwLock};

#[derive(Clone)]
pub struct Stdout {
    inner: Arc<RwLock<Option<io::BoxWrite>>>,
}

impl Stdout {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    pub fn set(&self, writer: io::BoxWrite) {
        *self.inner.write() = Some(writer);
    }

    pub fn print(&self, args: fmt::Arguments) {
        let mut stdout = self.inner.write();
        if let Some(stdout) = stdout.as_mut() {
            let _ = stdout.write_fmt(args);
        }
    }
}

impl StdoutWrite for Stdout {
    fn write_char(&self, ch: char) {
        let mut stdout = self.inner.write();
        if let Some(stdout) = stdout.as_mut() {
            let _ = stdout.write_char(ch);
        }
    }
}
