use core::{iter, ptr::NonNull};

use crate::{error::*, read::FdtReader, Fdt64, FdtHeader, FdtReserveEntry, MemoryRegion};

pub struct Fdt<'a> {
    pub header: FdtHeader,
    pub data: &'a [u8],
}

impl<'a> Fdt<'a> {
    pub fn from_bytes(data: &'a [u8]) -> FdtResult<Self> {
        let header = FdtHeader::from_bytes(data)?;

        header.valid_magic()?;

        Ok(Self { header, data })
    }

    pub fn from_ptr(ptr: NonNull<u8>) -> FdtResult<Self> {
        let tmp_header =
            unsafe { core::slice::from_raw_parts(ptr.as_ptr(), core::mem::size_of::<FdtHeader>()) };
        let real_size = FdtHeader::from_bytes(tmp_header)?.totalsize.get() as usize;

        Self::from_bytes(unsafe { core::slice::from_raw_parts(ptr.as_ptr(), real_size) })
    }

    fn reader<'b: 'a>(&'b self, offset: usize) -> FdtReader<'a, 'b> {
        FdtReader::new(&self.header, &self.data[offset..])
    }

    pub fn reserved_memory_regions(&self) -> impl Iterator<Item = MemoryRegion> + '_ {
        let mut reader = self.reader(self.header.off_mem_rsvmap.get() as _);
        iter::from_fn(move || match reader.reserved_memory() {
            Ok(region) => {
                if region.address.get() == 0 && region.size.get() == 0 {
                    return None;
                } else {
                    return Some(region.into());
                }
            }
            Err(_) => None,
        })
    }
}
