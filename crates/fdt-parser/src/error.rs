pub type FdtResult<T = ()> = Result<T, FdtError>;

#[derive(Debug)]
pub enum FdtError {
    /// The FDT had an invalid magic value.
    BadMagic,
    /// The given pointer was null.
    BadPtr,

    /// The slice passed in was too small to fit the given total size of the FDT
    /// structure.
    BufferTooSmall,
}
