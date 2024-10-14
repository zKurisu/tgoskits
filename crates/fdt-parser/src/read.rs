use core::ffi::CStr;

use crate::{
    error::{FdtError, FdtResult},
    Fdt32, Fdt64, FdtHeader, FdtReserveEntry, MemoryRegion, Token,
};

#[derive(Clone)]
pub(crate) struct FdtReader<'a, 'b: 'a> {
    header: &'b FdtHeader,
    bytes: &'a [u8],
}

impl<'a, 'b: 'a> FdtReader<'a, 'b> {
    pub fn new(header: &'b FdtHeader, bytes: &'a [u8]) -> Self {
        Self { header, bytes }
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

    pub fn take_prop(&mut self) -> Option<FdtProp> {
        let len = self.take_u32()?;
        let nameoff = self.take_u32()?;
        let bytes = self.take_aligned(len as _)?;
        Some(FdtProp {
            nameoff,
            data: FdtReader {
                header: &self.header,
                bytes,
            },
        })
    }
}

pub(crate) struct FdtProp<'a, 'b: 'a> {
    nameoff: u32,
    data: FdtReader<'a, 'b>,
}
