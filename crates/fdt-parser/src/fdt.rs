use core::{iter, ptr::NonNull};

use crate::{
    error::*, node::Node, read::FdtReader, Fdt64, FdtHeader, FdtReserveEntry, MemoryRegion, Token,
};

#[derive(Clone)]
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

    pub fn version(&self) -> usize {
        self.header.version.get() as _
    }

    pub fn reserved_memory_regions(&self) -> impl Iterator<Item = MemoryRegion> + '_ {
        let mut reader = self.reader(self.header.off_mem_rsvmap.get() as _);
        iter::from_fn(move || match reader.reserved_memory() {
            Some(region) => {
                if region.address == 0 && region.size == 0 {
                    return None;
                } else {
                    return Some(region.into());
                }
            }
            None => None,
        })
    }

    // fn get_string(&self, offset: usize) {}

    pub fn all_nodes<'b: 'a>(&'b self) -> impl Iterator<Item = Node<'a, 'b>> {
        let reader = self.reader(self.header.off_dt_struct.get() as _);
        FdtIter {
            fdt: self.clone(),
            level: 0,
            reader,
        }
    }
}

pub struct FdtIter<'a, 'b: 'a> {
    fdt: Fdt<'a>,
    level: usize,
    reader: FdtReader<'a, 'b>,
}

impl<'a, 'b: 'a> Iterator for FdtIter<'a, 'b> {
    type Item = Node<'a, 'b>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let token = self.reader.take_token()?;

            match token {
                Token::BeginNode => {
                    let node = Node::new(self.level as _, self.fdt.clone(), &mut self.reader);
                    self.level += 1;
                    return Some(node);
                }
                Token::EndNode => {
                    self.level -= 1;
                }
                Token::Prop => {
                    let _ = self.reader.take_prop();
                }
                Token::End => return None,
                _ => {}
            }
        }
    }
}
