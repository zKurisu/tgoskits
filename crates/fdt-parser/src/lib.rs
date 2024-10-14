#![no_std]

mod cell;
mod define;
pub mod error;
mod fdt;
mod node;
mod read;

use core::{ffi::CStr, ptr::NonNull};

use define::*;
use error::{FdtError, FdtResult};
use node::{FdtNode, NodeBytesIter};

pub use fdt::Fdt;

#[derive(Debug, Clone, Copy)]
pub struct FdtRef<'a> {
    data: &'a [u8],
    header: FdtHeader,
}

impl<'a> FdtRef<'a> {
    pub fn from_bytes(data: &'a [u8]) -> FdtResult<Self> {
        let header = FdtHeader::from_bytes(data)?;
        header.valid_magic()?;
        if data.len() < header.totalsize.get() as usize {
            return Err(FdtError::BufferTooSmall);
        }
        Ok(Self { data, header })
    }

    pub fn total_size(&self) -> usize {
        self.header.totalsize.get() as _
    }

    pub fn from_ptr(ptr: NonNull<u8>) -> FdtResult<Self> {
        let tmp_header =
            unsafe { core::slice::from_raw_parts(ptr.as_ptr(), core::mem::size_of::<FdtHeader>()) };
        let real_size = FdtHeader::from_bytes(tmp_header).unwrap().totalsize.get() as usize;

        Self::from_bytes(unsafe { core::slice::from_raw_parts(ptr.as_ptr(), real_size) })
    }
    pub(crate) fn cstr_at_offset(&self, offset: usize) -> &'a CStr {
        CStr::from_bytes_until_nul(self.strings_block().get(offset..).unwrap_or_default())
            .unwrap_or_default()
    }

    pub(crate) fn str_at_offset(&self, offset: usize) -> &'a str {
        self.cstr_at_offset(offset).to_str().unwrap_or_default()
    }
    fn strings_block(&self) -> &'a [u8] {
        self.data
            .get(self.header.strings_range())
            .unwrap_or_default()
    }

    fn structs_block(&self) -> &'a [u8] {
        self.data
            .get(self.header.struct_range())
            .unwrap_or_default()
    }

    pub fn all_nodes(&self) -> impl Iterator<Item = FdtNode<'a>> {
        NodeBytesIter::new(self.structs_block(), *self)
    }
}
