#![no_std]

mod cell;
mod define;
pub mod error;
mod node;

use core::{ffi::CStr, ptr::NonNull};

use define::*;
use error::{FdtError, FdtResult};
use node::{FdtNode, NodeBytesIter};

#[derive(Debug, Clone, Copy)]
pub struct FdtRef<'a> {
    data: &'a [u8],
    header: FdtHeader,
}

impl<'a> FdtRef<'a> {
    pub fn from_bytes(data: &'a [u8]) -> FdtResult<Self> {
        let header = FdtHeader::from_bytes(data).ok_or(FdtError::BufferTooSmall)?;
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

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct FdtHeader {
    /// FDT header magic
    magic: BigEndianU32,
    /// Total size in bytes of the FDT structure
    totalsize: BigEndianU32,
    /// Offset in bytes from the start of the header to the structure block
    off_dt_struct: BigEndianU32,
    /// Offset in bytes from the start of the header to the strings block
    off_dt_strings: BigEndianU32,
    /// Offset in bytes from the start of the header to the memory reservation
    /// block
    off_mem_rsvmap: BigEndianU32,
    /// FDT version
    version: BigEndianU32,
    /// Last compatible FDT version
    last_comp_version: BigEndianU32,
    /// System boot CPU ID
    boot_cpuid_phys: BigEndianU32,
    /// Length in bytes of the strings block
    size_dt_strings: BigEndianU32,
    /// Length in bytes of the struct block
    size_dt_struct: BigEndianU32,
}

impl FdtHeader {
    fn valid_magic(&self) -> FdtResult {
        if self.magic.get() == 0xd00dfeed {
            Ok(())
        } else {
            Err(FdtError::BadMagic)
        }
    }

    fn struct_range(&self) -> core::ops::Range<usize> {
        let start = self.off_dt_struct.get() as usize;
        let end = start + self.size_dt_struct.get() as usize;

        start..end
    }

    fn strings_range(&self) -> core::ops::Range<usize> {
        let start = self.off_dt_strings.get() as usize;
        let end = start + self.size_dt_strings.get() as usize;
        start..end
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let mut buff = ByteBuffer::new(bytes);
        Some(Self {
            magic: buff.u32()?,
            totalsize: buff.u32()?,
            off_dt_struct: buff.u32()?,
            off_dt_strings: buff.u32()?,
            off_mem_rsvmap: buff.u32()?,
            version: buff.u32()?,
            last_comp_version: buff.u32()?,
            boot_cpuid_phys: buff.u32()?,
            size_dt_strings: buff.u32()?,
            size_dt_struct: buff.u32()?,
        })
    }
}
