#[derive(Debug)]
pub enum DriverError {
    NotSupported,
}

pub type DriverResult<T> = core::result::Result<T, DriverError>;
