pub type Result<T = ()> = core::result::Result<T, Error>;

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    fn can_read(&self) -> bool;
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
    fn can_write(&self) -> bool;
}

#[derive(Debug, Copy, Clone)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: &'static str,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ErrorKind {
    /// Unspecified error kind.
    Other,

    /// An entity was not found, often a file.
    NotFound,
    /// The operation lacked the necessary privileges to complete.
    PermissionDenied,
    /// The connection was refused by the remote server.
    ConnectionRefused,
    /// The connection was reset by the remote server.
    ConnectionReset,
    /// The connection was aborted (terminated) by the remote server.
    ConnectionAborted,
    /// The network operation failed because it was not connected yet.
    NotConnected,
    /// A socket address could not be bound because the address is already in
    /// use elsewhere.
    AddrInUse,
    /// A nonexistent interface was requested or the requested address was not
    /// local.
    AddrNotAvailable,
    /// The operation failed because a pipe was closed.
    BrokenPipe,
    /// An entity already exists, often a file.
    AlreadyExists,
    /// A parameter was incorrect.
    InvalidInput,
    /// Data not valid for the operation were encountered.
    ///
    /// Unlike [`InvalidInput`], this typically means that the operation
    /// parameters were valid, however the error was caused by malformed
    /// input data.
    ///
    /// For example, a function that reads a file into a string will error with
    /// `InvalidData` if the file's contents are not valid UTF-8.
    ///
    /// [`InvalidInput`]: ErrorKind::InvalidInput
    InvalidData,
    /// The I/O operation's timeout expired, causing it to be canceled.
    TimedOut,
    /// This operation was interrupted.
    ///
    /// Interrupted operations can typically be retried.
    Interrupted,
    /// This operation is unsupported on this platform.
    ///
    /// This means that the operation can never succeed.
    Unsupported,
    /// An operation could not be completed, because it failed
    /// to allocate enough memory.
    OutOfMemory,
    /// An attempted write could not write any data.
    WriteZero,
}
