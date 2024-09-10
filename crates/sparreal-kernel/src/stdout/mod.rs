use alloc::boxed::Box;
use driver_interface::io;

use crate::sync::RwLock;

static STDOUT: RwLock<Option<Box<dyn io::Write>>> = RwLock::new(None);

pub(crate) fn set_stdout(stdout: Box<dyn io::Write>) {
    *STDOUT.write() = Some(stdout);
}
