use core::iter;

use crate::{
    meta::MetaData, property::Property, read::FdtReader, Fdt, FdtRange, FdtRangeSilce, FdtReg,
    Token,
};

#[derive(Clone)]
pub struct Node<'a> {
    pub level: usize,
    pub name: &'a str,
    fdt: &'a Fdt<'a>,
    pub(crate) meta_parent: MetaData<'a>,
    pub(crate) meta: MetaData<'a>,
    body: FdtReader<'a>,
}

impl<'a> Node<'a> {
    pub(crate) fn new(
        fdt: &'a Fdt<'a>,
        level: usize,
        name: &'a str,
        reader: FdtReader<'a>,
        meta_parent: MetaData<'a>,
        meta: MetaData<'a>,
    ) -> Self {
        Self {
            fdt,
            level,
            body: reader,
            name,
            meta,
            meta_parent,
        }
    }

    pub fn name(&self) -> &'a str {
        self.name
    }

    pub fn propertys(&self) -> impl Iterator<Item = Property<'a>> + '_ {
        let reader = self.body.clone();
        PropIter {
            reader,
            fdt: self.fdt,
        }
    }

    pub fn find_property(&self, name: &str) -> Option<Property<'a>> {
        self.propertys().find(|x| x.name.eq(name))
    }

    pub fn reg(&self) -> impl Iterator<Item = FdtReg> + 'a {
        let mut iter = self.propertys();
        let reg = iter.find(|x| x.name.eq("reg"));

        RegIter {
            address_cell: self.address_cells().unwrap(),
            size_cell: self.size_cells().unwrap(),
            prop: reg,
            node: self.clone(),
        }
    }

    fn address_cells(&self) -> Option<u8> {
        if let Some(a) = self.meta.address_cells {
            return Some(a);
        }
        self.meta_parent.address_cells
    }

    fn size_cells(&self) -> Option<u8> {
        if let Some(a) = self.meta.size_cells {
            return Some(a);
        }
        self.meta_parent.size_cells
    }

    // pub(crate) fn node_address_cells(&self) -> Option<u8> {
    //     self.find_property("#address-cells")?
    //         .data
    //         .take_u32()
    //         .map(|o| o as u8)
    // }

    // pub(crate) fn node_size_cells(&self) -> Option<u8> {
    //     self.find_property("#size-cells")?
    //         .data
    //         .take_u32()
    //         .map(|o| o as u8)
    // }

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
            self.meta_parent.address_cells.unwrap(),
            self.meta.size_cells.unwrap(),
            prop.data.clone(),
        ))
    }
}

struct RegIter<'a> {
    address_cell: u8,
    size_cell: u8,
    prop: Option<Property<'a>>,
    node: Node<'a>,
}
impl<'a> Iterator for RegIter<'a> {
    type Item = FdtReg;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(prop) = &mut self.prop {
            let child_address_cell = self.node.address_cells().unwrap();
            let child_bus_address = prop.data.take_by_cell_size(child_address_cell)?;

            let mut address = child_bus_address;
            for one in self.node.ranges() {
                if child_bus_address >= one.child_bus_address
                    && child_bus_address < one.child_bus_address + one.size as u128
                {
                    address = child_bus_address - one.child_bus_address + one.parent_bus_address;
                }
            }

            let size = if self.size_cell > 0 {
                Some(prop.data.take_by_cell_size(self.size_cell)? as usize)
            } else {
                None
            };
            Some(FdtReg {
                address,
                child_bus_address,
                size,
            })
        } else {
            None
        }
    }
}

struct PropIter<'a> {
    fdt: &'a Fdt<'a>,
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
        self.reader.take_prop(self.fdt)
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
