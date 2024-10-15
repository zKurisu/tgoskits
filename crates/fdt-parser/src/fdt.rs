use core::{ffi::CStr, iter, ptr::NonNull};

use crate::{
    cell::MetaData, error::*, node::Node, read::FdtReader, FdtHeader, MemoryRegion, Token,
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
        FdtReader::new(self, &self.data[offset..])
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

    pub(crate) fn get_str<'b: 'a>(&'b self, offset: usize) -> FdtResult<&'a str> {
        let reader = self.reader(self.header.off_dt_strings.get() as usize + offset);
        let s = CStr::from_bytes_until_nul(reader.remaining())
            .map_err(|_e| FdtError::Utf8Parse)?
            .to_str()?;
        Ok(s)
    }

    pub fn all_nodes<'b: 'a>(&'b self) -> impl Iterator<Item = Node<'a, 'b>> {
        let reader = self.reader(self.header.off_dt_struct.get() as _);
        FdtIter {
            fdt: self.clone(),
            parent_index: 0,
            reader,
            stack: Default::default(),
            meta: Default::default(),
        }
    }
}

#[derive(Default)]
struct MetaStack {
    address_cells: [Option<u8>; 12],
    size_cells: [Option<u8>; 12],
    clock_cells: [Option<u8>; 12],
    interrupt_cells: [Option<u8>; 12],
    gpio_cells: [Option<u8>; 12],
    dma_cells: [Option<u8>; 12],
    cooling_cells: [Option<u8>; 12],
}

pub struct FdtIter<'a, 'b: 'a> {
    fdt: Fdt<'a>,
    parent_index: usize,
    reader: FdtReader<'a, 'b>,
    stack: MetaStack,
    meta: MetaData,
}

impl<'a, 'b: 'a> FdtIter<'a, 'b> {
    fn get_meta(&self) -> MetaData {
        let mut meta = MetaData::default();
        macro_rules! get_size {
            ($cell:ident) => {{
                let mut size = 0;
                for i in (0..self.parent_index).rev() {
                    if let Some(cell_size) = self.stack.$cell[i] {
                        size = cell_size;
                        break;
                    }
                }
                meta.$cell = size;
            }};
        }

        get_size!(address_cells);
        get_size!(size_cells);
        get_size!(clock_cells);
        get_size!(interrupt_cells);
        get_size!(gpio_cells);
        get_size!(dma_cells);
        get_size!(cooling_cells);

        meta
    }

    fn next_level(&mut self) {
        self.parent_index += 1;
        let i = self.parent_index;
        self.stack.address_cells[i] = None;
        self.stack.size_cells[i] = None;
        self.stack.clock_cells[i] = None;
        self.stack.interrupt_cells[i] = None;
        self.stack.gpio_cells[i] = None;
        self.stack.dma_cells[i] = None;
        self.stack.cooling_cells[i] = None;
    }
}

impl<'a, 'b: 'a> Iterator for FdtIter<'a, 'b> {
    type Item = Node<'a, 'b>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let token = self.reader.take_token()?;

            match token {
                Token::BeginNode => {
                    let mut node = Node::new(self.parent_index as _, &mut self.reader);
                    self.next_level();
                    for prop in node.propertys() {
                        macro_rules! update_cell {
                            ($cell:ident) => {
                                self.stack.$cell[self.parent_index] = Some(prop.u32() as _)
                            };
                        }
                        match prop.name {
                            "#address-cells" => update_cell!(address_cells),
                            "#size-cells" => update_cell!(size_cells),
                            "#clock-cells" => update_cell!(clock_cells),
                            "#interrupt-cells" => update_cell!(interrupt_cells),
                            "#gpio-cells" => update_cell!(gpio_cells),
                            "#dma-cells" => update_cell!(dma_cells),
                            "#cooling-cells" => update_cell!(cooling_cells),
                            _ => {}
                        }
                    }
                    node.meta = self.get_meta();
                    return Some(node);
                }
                Token::EndNode => {
                    self.parent_index -= 1;
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
