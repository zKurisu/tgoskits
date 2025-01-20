use alloc::string::String;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DriverError {
    #[error("Already used by `{0}`")]
    UsedByOthers(String),
}
