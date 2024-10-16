use core::iter;

use crate::meta::MetaData;
use crate::read::{FdtReader, Property};
use crate::{Cell, CellSilceIter, Fdt, FdtRange, FdtRangeSilce, MemoryRegion, Reg, Token};

#[derive(Clone)]
pub struct Node<'a> {
    pub level: usize,
    pub name: &'a str,
    pub(crate) meta: MetaData<'a>,
    body: FdtReader<'a>,
}

impl<'a> Node<'a> {
    pub(crate) fn new(level: usize, reader: &mut FdtReader<'a>) -> Self {
        let name = reader.take_unit_name().unwrap();

        Self {
            level,
            body: reader.clone(),
            name,
            meta: MetaData::default(),
        }
    }

    pub fn name(&self) -> &'a str {
        self.name
    }

    pub fn propertys(&self) -> impl Iterator<Item = Property<'a>> + '_ {
        let reader = self.body.clone();
        PropIter { reader }
    }

    pub fn find_property(&self, name: &str) -> Option<Property<'a>> {
        self.propertys().find(|x| x.name.eq(name))
    }

    pub fn reg(&self) -> impl Iterator<Item = Reg> + 'a {
        let mut iter = self.propertys();
        let reg = iter.find(|x| x.name.eq("reg"));

        let address_cell = self.meta.address_cells.unwrap();
        let size_cell = self.meta.size_cells.unwrap();

        RegIter {
            address_cell,
            size_cell,
            prop: reg,
        }
    }

    pub fn ranges(&self) -> impl Iterator<Item = FdtRange> + 'a {
        let mut iter = self.meta.range.clone().map(|m| m.iter());
        iter::from_fn(move || match &mut iter {
            Some(i) => i.next(),
            None => None,
        })
    }

    pub(crate) fn node_ranges(&self) -> Option<FdtRangeSilce<'a>> {
        let prop = self.find_property("ranges")?;
        Some(FdtRangeSilce::new(
            self.meta.address_cells.unwrap(),
            self.meta.size_cells.unwrap(),
            prop.data.clone(),
        ))
    }
}

struct RegIter<'a> {
    address_cell: u8,
    size_cell: u8,
    prop: Option<Property<'a>>,
}
impl<'a> Iterator for RegIter<'a> {
    type Item = Reg;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(prop) = &mut self.prop {
            let address_bytes_num = self.address_cell as usize * 4;
            let address = prop.data.take(address_bytes_num)?;
            let size = if self.size_cell > 0 {
                Some(prop.data.take_by_cell_size(self.size_cell)?)
            } else {
                None
            };
            Some(Reg::new(self.address_cell, address, size))
        } else {
            None
        }
    }
}

struct PropIter<'a> {
    reader: FdtReader<'a>,
}

impl<'a> Iterator for PropIter<'a> {
    type Item = Property<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.reader.take_token() {
                Some(token) => match token {
                    Token::Prop => break,
                    Token::Nop => {}
                    _ => return None,
                },
                None => return None,
            }
        }
        self.reader.take_prop()
    }
}

#[derive(Clone)]
pub struct MemoryRegionSilce<'a> {
    address_cell: u8,
    size_cell: u8,
    reader: FdtReader<'a>,
}

impl<'a> MemoryRegionSilce<'a> {
    pub fn iter(&self) -> impl Iterator<Item = FdtRange> + 'a {
        MemoryRegionIter {
            address_cell: self.address_cell,
            size_cell: self.size_cell,
            reader: self.reader.clone(),
        }
    }
}

struct MemoryRegionIter<'a> {
    address_cell: u8,
    size_cell: u8,
    reader: FdtReader<'a>,
}

impl<'a> Iterator for MemoryRegionIter<'a> {
    type Item = FdtRange;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}
