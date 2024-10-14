use core::fmt::{Debug, Display};

use crate::error::*;

const FDT_TAGSIZE: usize = size_of::<Fdt32>();

pub const FDT_BEGIN_NODE: u32 = 1;
pub const FDT_END_NODE: u32 = 2;
pub const FDT_PROP: u32 = 3;
pub const FDT_NOP: u32 = 4;
pub const FDT_END: u32 = 5;

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Fdt32(u32);

impl Fdt32 {
    pub const fn new() -> Self {
        Self(0)
    }

    pub fn get(self) -> u32 {
        self.0
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        Some(Fdt32(u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?)))
    }
}

impl From<&[u8]> for Fdt32 {
    fn from(value: &[u8]) -> Self {
        Fdt32(u32::from_be_bytes(value.try_into().unwrap()))
    }
}

impl Default for Fdt32 {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct Fdt64(u64);

impl Fdt64 {
    pub const fn new() -> Self {
        Self(0)
    }

    pub fn get(&self) -> u64 {
        self.0
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        Some(Fdt64(u64::from_be_bytes(bytes.get(..8)?.try_into().ok()?)))
    }
}
impl From<&[u8]> for Fdt64 {
    fn from(value: &[u8]) -> Self {
        Fdt64(u64::from_be_bytes(value.try_into().unwrap()))
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

    pub(crate) fn from_bytes(bytes: &[u8]) -> FdtResult<Self> {
        Self::_from_bytes(bytes).ok_or(FdtError::BufferTooSmall)
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
    pub address: Fdt64,
    pub size: Fdt64,
}
impl FdtReserveEntry {
    pub fn new(address: Fdt64, size: Fdt64) -> Self {
        Self { address, size }
    }
}

impl From<FdtReserveEntry> for MemoryRegion {
    fn from(value: FdtReserveEntry) -> Self {
        Self {
            address: value.address.get() as usize as _,
            size: value.size.get() as _,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub address: *mut u8,
    pub size: usize,
}
