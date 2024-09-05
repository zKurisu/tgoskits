#![no_std]

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

mod table;

pub use table::*;
pub use memory_addr::*;


/// The error type for page table operation failures.
#[derive(Debug, PartialEq)]
pub enum PagingError {
    /// Cannot allocate memory.
    NoMemory,
    /// The address is not aligned to the page size.
    NotAligned,
    /// The mapping is not present.
    NotMapped,
    /// The mapping is already present.
    AlreadyMapped,
    /// The page table entry represents a huge page, but the target physical
    /// frame is 4K in size.
    MappedToHugePage,
}

/// The specialized `Result` type for page table operations.
pub type PagingResult<T = ()> = Result<T, PagingError>;