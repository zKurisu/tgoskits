use core::{
    fmt::{Debug, Display},
    ptr::NonNull,
};

use crate::error::*;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Token {
    BeginNode,
    EndNode,
    Prop,
    Nop,
    End,
    Data,
}
impl From<u32> for Token {
    fn from(value: u32) -> Self {
        match value {
            0x1 => Token::BeginNode,
            0x2 => Token::EndNode,
            0x3 => Token::Prop,
            0x4 => Token::Nop,
            0x9 => Token::End,
            _ => Token::Data,
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Fdt32([u8; 4]);

impl Fdt32 {
    pub const fn new() -> Self {
        Self([0; 4])
    }

    pub fn get(self) -> u32 {
        u32::from_be_bytes(self.0)
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        Some(Self(bytes.get(..4)?.try_into().ok()?))
    }
}

impl From<&[u8]> for Fdt32 {
    fn from(value: &[u8]) -> Self {
        Fdt32(value.get(..4).unwrap().try_into().unwrap())
    }
}

impl Default for Fdt32 {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Fdt64([u8; 8]);

impl Fdt64 {
    pub const fn new() -> Self {
        Self([0; 8])
    }

    pub fn get(&self) -> u64 {
        u64::from_be_bytes(self.0)
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        Some(Self(bytes.get(..8)?.try_into().ok()?))
    }
}
impl From<&[u8]> for Fdt64 {
    fn from(value: &[u8]) -> Self {
        Self(value.get(..8).unwrap().try_into().unwrap())
    }
}
impl Default for Fdt64 {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ByteBuffer<'a> {
    bytes: &'a [u8],
}

impl<'a> ByteBuffer<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub fn u32(&mut self) -> Option<Fdt32> {
        let bytes = self.take(4)?;
        Fdt32::from_bytes(bytes)
    }

    pub fn u64(&mut self) -> Option<Fdt64> {
        let bytes = self.take(8)?;
        Fdt64::from_bytes(bytes)
    }

    pub fn skip(&mut self, n_bytes: usize) -> Option<()> {
        self.bytes = self.bytes.get(n_bytes..)?;
        Some(())
    }

    pub fn remaining(&self) -> &'a [u8] {
        self.bytes
    }

    pub fn peek_u32(&self) -> Option<Fdt32> {
        let bytes = self.bytes.get(..4)?;
        Fdt32::from_bytes(bytes)
    }

    pub fn is_empty(&self) -> bool {
        self.remaining().is_empty()
    }

    pub fn take(&mut self, bytes: usize) -> Option<&'a [u8]> {
        if self.bytes.len() >= bytes {
            let ret = self.bytes.get(..bytes)?;
            self.skip(bytes);
            return Some(ret);
        }
        None
    }
    pub fn skip_4_aligned(&mut self, len: usize) {
        self.skip((len + 3) & !0x3);
    }
}

/// A raw `reg` property value set
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawReg<'a> {
    /// Big-endian encoded bytes making up the address portion of the property.
    /// Length will always be a multiple of 4 bytes.
    pub address: &'a [u8],
    /// Big-endian encoded bytes making up the size portion of the property.
    /// Length will always be a multiple of 4 bytes.
    pub size: &'a [u8],
}
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub(crate) struct FdtHeader {
    /// FDT header magic
    pub magic: Fdt32,
    /// Total size in bytes of the FDT structure
    pub totalsize: Fdt32,
    /// Offset in bytes from the start of the header to the structure block
    pub off_dt_struct: Fdt32,
    /// Offset in bytes from the start of the header to the strings block
    pub off_dt_strings: Fdt32,
    /// Offset in bytes from the start of the header to the memory reservation
    /// block
    pub off_mem_rsvmap: Fdt32,
    /// FDT version
    pub version: Fdt32,
    /// Last compatible FDT version
    pub last_comp_version: Fdt32,
    /// System boot CPU ID
    pub boot_cpuid_phys: Fdt32,
    /// Length in bytes of the strings block
    pub size_dt_strings: Fdt32,
    /// Length in bytes of the struct block
    pub size_dt_struct: Fdt32,
}

impl FdtHeader {
    pub(crate) fn valid_magic(&self) -> FdtResult {
        if self.magic.get() == 0xd00dfeed {
            Ok(())
        } else {
            Err(FdtError::BadMagic)
        }
    }

    pub(crate) fn struct_range(&self) -> core::ops::Range<usize> {
        let start = self.off_dt_struct.get() as usize;
        let end = start + self.size_dt_struct.get() as usize;

        start..end
    }

    pub(crate) fn strings_range(&self) -> core::ops::Range<usize> {
        let start = self.off_dt_strings.get() as usize;
        let end = start + self.size_dt_strings.get() as usize;
        start..end
    }

    fn _from_bytes(bytes: &[u8]) -> Option<Self> {
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

    pub fn from_bytes(bytes: &[u8]) -> FdtResult<Self> {
        // Self::_from_bytes(bytes).ok_or(FdtError::BufferTooSmall)

        if bytes.len() < size_of::<FdtHeader>() {
            return Err(FdtError::BufferTooSmall);
        }

        unsafe {
            let ptr: *const FdtHeader = bytes.as_ptr().cast();
            Ok(ptr.read())
        }
    }

    pub fn from_ptr(ptr: NonNull<u8>) -> FdtResult<Self> {
        let ptr: NonNull<FdtHeader> = ptr.cast();
        unsafe { Ok(ptr.as_ref().clone()) }
    }
}

impl Display for FdtHeader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FdtHeader")
            .field("size", &self.totalsize.get())
            .field("version", &self.version.get())
            .field("last_comp_version", &self.last_comp_version.get())
            .finish()
    }
}

#[repr(C)]
pub(crate) struct FdtReserveEntry {
    pub address: u64,
    pub size: u64,
}
impl FdtReserveEntry {
    pub fn new(address: u64, size: u64) -> Self {
        Self { address, size }
    }
}

impl From<FdtReserveEntry> for MemoryRegion {
    fn from(value: FdtReserveEntry) -> Self {
        Self {
            address: value.address as usize as _,
            size: value.size as _,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub address: *mut u8,
    pub size: usize,
}
