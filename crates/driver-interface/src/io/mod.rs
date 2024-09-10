pub type IOError = embedded_io::ErrorKind;
pub type IOResult<T = ()> = Result<T, IOError>;

pub trait Write: Send + Sync{
    // Required methods
    fn write(&mut self, buf: &[u8]) -> IOResult<usize>;
    fn flush(&mut self) -> IOResult;

    /// Write an entire buffer into this writer.
    ///
    /// This function calls `write()` in a loop until exactly `buf.len()` bytes have
    /// been written, blocking if needed.
    ///
    /// If you are using [`WriteReady`] to avoid blocking, you should not use this function.
    /// `WriteReady::write_ready()` returning true only guarantees the first call to `write()` will
    /// not block, so this function may still block in subsequent calls.
    ///
    /// This function will panic if `write()` returns `Ok(0)`.
    fn write_all(&mut self, mut buf: &[u8]) -> IOResult {
        while !buf.is_empty() {
            match self.write(buf) {
                Ok(0) => panic!("write() returned Ok(0)"),
                Ok(n) => buf = &buf[n..],
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
