use crate::{
    error::{FdtError, FdtResult},
    Fdt32, Fdt64, FdtHeader, FdtReserveEntry, MemoryRegion,
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

    pub fn take_u32(&mut self) -> FdtResult<Fdt32> {
        let bytes = self.take(4)?;
        Ok(bytes.into())
    }

    pub fn take_u64(&mut self) -> FdtResult<Fdt64> {
        let bytes = self.take(8)?;
        Ok(bytes.into())
    }

    pub fn skip(&mut self, n_bytes: usize) -> FdtResult {
        self.bytes = self.bytes.get(n_bytes..).ok_or(FdtError::BufferTooSmall)?;
        Ok(())
    }

    pub fn remaining(&self) -> &'a [u8] {
        self.bytes
    }

    pub fn u32(&self) -> FdtResult<Fdt32> {
        let bytes = self.bytes.get(..4).ok_or(FdtError::BufferTooSmall)?;
        Ok(bytes.into())
    }

    pub fn is_empty(&self) -> bool {
        self.remaining().is_empty()
    }

    pub fn take(&mut self, bytes: usize) -> FdtResult<&'a [u8]> {
        if self.bytes.len() >= bytes {
            let ret = self.bytes.get(..bytes).ok_or(FdtError::BufferTooSmall)?;
            self.skip(bytes);
            return Ok(ret);
        }
        Err(FdtError::BufferTooSmall)
    }
    pub fn skip_4_aligned(&mut self, len: usize) -> FdtResult {
        self.skip((len + 3) & !0x3)
    }

    pub fn reserved_memory(&mut self) -> FdtResult<FdtReserveEntry> {
        let address = self.take_u64()?;
        let size = self.take_u64()?;
        Ok(FdtReserveEntry::new(address, size))
    }
}
