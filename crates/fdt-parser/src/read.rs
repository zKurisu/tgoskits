use core::ffi::CStr;

use crate::{
    error::{FdtError, FdtResult},
    Fdt, Fdt32, Fdt64, FdtReserveEntry, Token,
};

#[derive(Clone)]
pub(crate) struct FdtReader<'a, 'b: 'a> {
    pub fdt: &'b Fdt<'a>,
    bytes: &'a [u8],
}

impl<'a, 'b: 'a> FdtReader<'a, 'b> {
    pub fn new(fdt: &'a Fdt<'a>, bytes: &'a [u8]) -> Self {
        Self { fdt, bytes }
    }

    pub fn take_u32(&mut self) -> Option<u32> {
        let bytes = self.take(4)?;
        let fdt32: Fdt32 = bytes.into();
        Some(fdt32.get())
    }

    pub fn take_u64(&mut self) -> Option<u64> {
        let bytes = self.take(8)?;
        let fdt64: Fdt64 = bytes.into();
        Some(fdt64.get())
    }

    pub fn take_by_cell_size(&mut self, cell_size: u8) -> Option<usize> {
        match cell_size {
            1 => self.take_u32().map(|s| s as usize),
            2 => self.take_u64().map(|s| s as usize),
            _ => panic!("invalid cell size {}", cell_size),
        }
    }

    pub fn skip(&mut self, n_bytes: usize) -> FdtResult {
        self.bytes = self.bytes.get(n_bytes..).ok_or(FdtError::BufferTooSmall)?;
        Ok(())
    }

    pub fn remaining(&self) -> &'a [u8] {
        self.bytes
    }

    // pub fn u32(&self) -> FdtResult<u32> {
    //     let bytes = self.bytes.get(..4).ok_or(FdtError::BufferTooSmall)?;
    //     let fdt32: Fdt32 = bytes.into();
    //     Ok(fdt32.get())
    // }

    pub fn take_token(&mut self) -> Option<Token> {
        let u = self.take_u32()?;
        Some(Token::from(u))
    }

    pub fn is_empty(&self) -> bool {
        self.remaining().is_empty()
    }

    pub fn take(&mut self, bytes: usize) -> Option<&'a [u8]> {
        if bytes == 0 {
            return Some(&[]);
        }

        if self.bytes.len() >= bytes {
            let ret = self.bytes.get(..bytes)?;
            let _ = self.skip(bytes);
            return Some(ret);
        }
        None
    }

    pub fn take_by(&mut self, offset: usize) -> Option<Self> {
        let bytes = self.take(offset)?;
        Some(FdtReader::new(self.fdt, bytes))
    }

    pub fn take_aligned(&mut self, len: usize) -> Option<&'a [u8]> {
        let bytes = (len + 3) & !0x3;
        self.take(bytes)
    }

    pub fn skip_4_aligned(&mut self, len: usize) -> FdtResult {
        self.skip((len + 3) & !0x3)
    }

    pub fn reserved_memory(&mut self) -> Option<FdtReserveEntry> {
        let address = self.take_u64()?;
        let size = self.take_u64()?;
        Some(FdtReserveEntry::new(address, size))
    }

    pub fn take_unit_name(&mut self) -> FdtResult<&'a str> {
        let unit_name = CStr::from_bytes_until_nul(self.remaining())
            .map_err(|_e| FdtError::Utf8Parse)?
            .to_str()?;
        let full_name_len = unit_name.len() + 1;
        let _ = self.skip_4_aligned(full_name_len);
        Ok(if unit_name.is_empty() { "/" } else { unit_name })
    }

    pub fn take_prop(&mut self) -> Option<Property<'a, 'b>> {
        let len = self.take_u32()?;
        let nameoff = self.take_u32()?;
        let bytes = self.take_aligned(len as _)?;
        Some(Property {
            name: self.fdt.get_str(nameoff as _).unwrap_or("<error>"),
            data: FdtReader {
                fdt: &self.fdt,
                bytes,
            },
        })
    }
}

pub struct Property<'a, 'b: 'a> {
    pub name: &'a str,
    pub(crate) data: FdtReader<'a, 'b>,
}

impl<'a, 'b: 'a> Property<'a, 'b> {
    pub fn raw_value(&self) -> &'a [u8] {
        self.data.remaining()
    }

    pub fn u32(&self) -> u32 {
        self.data.clone().take_u32().unwrap()
    }
}
