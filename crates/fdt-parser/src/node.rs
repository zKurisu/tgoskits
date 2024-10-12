use core::ffi::CStr;

use crate::cell::{CellSize, CellSizes};
use crate::error::FdtResult;
use crate::{define::*, FdtRef};
use crate::{error::FdtError, ByteBuffer, FdtHeader};

/// A devicetree node
#[derive(Debug, Clone, Copy)]
pub struct FdtNode<'a> {
    pub name: &'a str,
    fdt: FdtRef<'a>,
    pub level: u32,
    props: &'a [u8],
    // parent_props: Option<&'a [u8]>,
}

impl<'a> FdtNode<'a> {
    /// Returns an iterator over the available properties of the node
    pub fn properties(self) -> impl Iterator<Item = NodeProperty<'a>> + 'a {
        let mut stream = ByteBuffer::new(self.props);
        let mut done = false;

        core::iter::from_fn(move || {
            if stream.is_empty() || done {
                return None;
            }

            while stream.peek_u32()?.get() == FDT_NOP {
                stream.skip(4);
            }

            if stream.peek_u32()?.get() == FDT_PROP {
                NodeProperty::parse(&mut stream, self.fdt).ok()
            } else {
                done = true;
                None
            }
        })
    }
}

pub struct NodeBytesIter<'a> {
    done: bool,
    buff: ByteBuffer<'a>,
    fdt: FdtRef<'a>,
    parent_index: u32,
}

impl<'a> NodeBytesIter<'a> {
    pub fn new(bytes: &'a [u8], fdt: FdtRef<'a>) -> Self {
        Self {
            done: false,
            buff: ByteBuffer::new(bytes),
            fdt,
            parent_index: 0,
        }
    }
}

impl<'a> Iterator for NodeBytesIter<'a> {
    type Item = FdtNode<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.buff.u32()?.get() {
                FDT_END_NODE => {
                    self.parent_index -= 1;
                }
                FDT_BEGIN_NODE => {
                    break;
                }
                FDT_END => {
                    self.done = true;
                    return None;
                }
                _ => {}
            }
        }

        let unit_name = CStr::from_bytes_until_nul(self.buff.remaining())
            .ok()?
            .to_str()
            .ok()?;
        let full_name_len = unit_name.len() + 1;
        self.buff.skip_4_aligned(full_name_len);
        let curr_node = self.buff.remaining();

        let level = self.parent_index;

        self.parent_index += 1;
        while self.buff.peek_u32()?.get() == FDT_NOP {
            self.buff.skip(4);
        }

        while self.buff.peek_u32()?.get() == FDT_PROP {
            NodeProperty::parse(&mut self.buff, self.fdt).ok()?;
        }

        Some(FdtNode {
            name: if unit_name.is_empty() { "/" } else { unit_name },
            fdt: self.fdt,
            level,
            props: curr_node,
        })
    }
}

/// A node property
#[derive(Debug, Clone, Copy)]
pub struct NodeProperty<'a> {
    /// Property name
    pub name: &'a str,
    /// Property value
    pub value: &'a [u8],
}

impl<'a> NodeProperty<'a> {
    /// Attempt to parse the property value as a `usize`
    pub fn as_usize(self) -> Option<usize> {
        match self.value.len() {
            4 => BigEndianU32::from_bytes(self.value).map(|i| i.get() as usize),
            8 => BigEndianU64::from_bytes(self.value).map(|i| i.get() as usize),
            _ => None,
        }
    }

    /// Attempts to parse the property value as a list of [`u64`].
    ///
    /// Only handles property values with uniform cell sizes.
    ///
    /// For `prop-encoded-array` property values use [iter_prop_encoded](Self::iter_prop_encoded).
    pub fn iter_cell_size(self, cell_size: CellSize) -> impl Iterator<Item = u64> + 'a {
        let mut cells = ByteBuffer::new(self.value);

        core::iter::from_fn(move || match cell_size {
            CellSize::One => Some(cells.u32()?.get() as u64),
            CellSize::Two => Some(cells.u64()?.get()),
            _ => None,
        })
    }

    /// Attempts to parse the property value as a `prop-encoded-array` list of [`u64`] tuples.
    pub fn iter_prop_encoded(self, cell_sizes: CellSizes) -> impl Iterator<Item = (u64, u64)> + 'a {
        let mut cells = ByteBuffer::new(self.value);

        core::iter::from_fn(move || {
            let addr = match cell_sizes.address_cells {
                CellSize::One => Some(cells.u32()?.get() as u64),
                CellSize::Two => Some(cells.u64()?.get()),
                _ => None,
            }?;

            let size = match cell_sizes.size_cells {
                CellSize::One => Some(cells.u32()?.get() as u64),
                CellSize::Two => Some(cells.u64()?.get()),
                _ => None,
            }?;

            Some((addr, size))
        })
    }

    /// Attempt to parse the property value as a `&str`
    pub fn as_str(self) -> Option<&'a str> {
        core::str::from_utf8(self.value)
            .map(|s| s.trim_end_matches('\0'))
            .ok()
    }

    /// Attempts to parse the property value as a list of [`&str`].
    pub fn iter_str(self) -> impl Iterator<Item = &'a str> + 'a {
        let mut strs = self.as_str().map(|s| s.split('\0'));

        core::iter::from_fn(move || match strs.as_mut() {
            Some(s) => s.next(),
            None => None,
        })
    }

    fn parse(stream: &mut ByteBuffer<'a>, fdt: FdtRef<'a>) -> FdtResult<Self> {
        match stream.u32() {
            Some(p) if p.get() == FDT_PROP => Ok(()),
            Some(other) => Err(FdtError::BadPropTag((other.get(), FDT_PROP))),
            None => Err(FdtError::BadPropTag((0, FDT_PROP))),
        }?;

        let prop = stream.fdt_property().ok_or(FdtError::MissingProperty)?;
        let data_len = prop.len.get() as usize;

        let data = stream.remaining().get(..data_len).unwrap_or_default();

        stream.skip_4_aligned(data_len);

        Ok(NodeProperty {
            name: fdt.str_at_offset(prop.name_offset.get() as usize),
            value: data,
        })
    }
}
#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct FdtProperty {
    len: BigEndianU32,
    name_offset: BigEndianU32,
}

impl ByteBuffer<'_> {
    fn fdt_property(&mut self) -> Option<FdtProperty> {
        let len = self.u32()?;
        let name_offset = self.u32()?;

        Some(FdtProperty { len, name_offset })
    }
}
