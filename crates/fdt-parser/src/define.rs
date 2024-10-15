use core::{
    fmt::{Debug, Display},
    ptr::{slice_from_raw_parts, NonNull},
};

use crate::{error::*, read::FdtReader};

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

#[derive(Clone, Copy)]
pub struct MemoryRegion {
    pub address: *mut u8,
    pub size: usize,
}

impl Debug for MemoryRegion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!(
            "MemoryRegion {{ address: {:p}, size: {:#x} }}",
            self.address, self.size
        ))
    }
}

#[derive(Clone)]
pub struct Cell<'a, 'b: 'a> {
    address_cell: u8,
    size_cell: u8,
    data: FdtReader<'a, 'b>,
}

impl<'a, 'b: 'a> IntoIterator for Cell<'a, 'b> {
    type Item = usize;

    type IntoIter = CellIter<'a, 'b>;

    fn into_iter(self) -> Self::IntoIter {
        CellIter::new(self.address_cell, self.size_cell, self.data.clone())
    }
}

#[derive(Clone)]
pub struct CellIter<'a, 'b: 'a> {
    address_cell: u8,
    size_cell: u8,
    i: usize,
    data: FdtReader<'a, 'b>,
}

impl<'a, 'b: 'a> CellIter<'a, 'b> {
    pub(crate) fn new(address_cell: u8, size_cell: u8, data: FdtReader<'a, 'b>) -> Self {
        Self {
            address_cell,
            size_cell,
            data,
            i: 0,
        }
    }
}

impl<'a, 'b: 'a> Iterator for CellIter<'a, 'b> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let one = self.data.take_by_cell_size(self.size_cell)?;
        self.i += 1;
        Some(one)
    }
}

pub struct CellSilceIter<'a, 'b: 'a> {
    address_cell: u8,
    size_cell: u8,
    data: FdtReader<'a, 'b>,
}

impl<'a, 'b: 'a> CellSilceIter<'a, 'b> {
    pub(crate) fn new(address_cell: u8, size_cell: u8, data: FdtReader<'a, 'b>) -> Self {
        Self {
            address_cell,
            size_cell,
            data,
        }
    }
}

impl<'a, 'b: 'a> Iterator for CellSilceIter<'a, 'b> {
    type Item = Cell<'a, 'b>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.data.is_empty() {
            return None;
        }

        let offset = self.address_cell as usize * 4 + self.size_cell as usize * 4;
        let data = self.data.take_by(offset)?;

        Some(Cell {
            address_cell: self.address_cell,
            size_cell: self.size_cell,
            data,
        })
    }
}
#[derive(Clone, Copy)]
pub struct Reg {
    address_cell: u8,
    pub(crate) address: [u8; 12],
    pub size: Option<usize>,
}

impl Reg {
    pub(crate) fn new(address_cell: u8, address: &[u8], size: Option<usize>) -> Self {
        let len = address_cell as usize * 4;
        let mut addr = [0; 4 * 3];
        addr[..len].copy_from_slice(&address[..len]);
        Self {
            address: addr,
            size,
            address_cell,
        }
    }

    pub fn address_raw(&self) -> [u32; 3] {
        let ptr: *const u32 = self.address.as_ptr().cast();
        let mut ret = [0; 3];
        let src = unsafe { &*slice_from_raw_parts(ptr, 3) };
        ret.copy_from_slice(src);
        ret
    }

    pub fn address(&self) -> usize {
        match self.address_cell {
            1 => {
                let fdt32: Fdt32 = self.address[..4].into();
                fdt32.get() as usize
            }
            2 => {
                let fdt64: Fdt64 = self.address[..8].into();
                fdt64.get() as usize
            }
            _ => panic!(
                "address-cell is {}, should use [address_raw]",
                self.address_cell
            ),
        }
    }
}

impl Debug for Reg {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.address_cell > 2 {
            f.write_fmt(format_args!("<{:?}, ", self.address_raw()))?;
        } else {
            f.write_fmt(format_args!("<{:#x}, ", self.address()))?;
        }

        if let Some(s) = self.size {
            f.write_fmt(format_args!("{:#x}>", s))
        } else {
            f.write_str("None>")
        }
    }
}

/// Range mapping child bus addresses to parent bus addresses
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FdtRange {
    /// Starting address on child bus
    pub child_bus_address: usize,
    /// The high bits of the child bus' starting address, if present
    pub child_bus_address_hi: u32,
    /// Starting address on parent bus
    pub parent_bus_address: usize,
    /// Size of range
    pub size: usize,
}


