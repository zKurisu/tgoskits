use alloc::string::String;

#[derive(Debug)]
pub enum DriverError {
    NotSupported,
    Other(String),
}

pub type DriverResult<T = ()> = core::result::Result<T, DriverError>;
