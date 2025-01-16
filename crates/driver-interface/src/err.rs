#[derive(Debug)]
pub enum DriverError {
    NotSupported,
}

pub type DruverResult<T> = core::result::Result<T, DriverError>;
