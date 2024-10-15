use core::iter;

use crate::cell::MetaData;
use crate::read::{FdtReader, Property};
use crate::{Fdt, Token};

#[derive(Clone)]
pub struct Node<'a, 'b: 'a> {
    pub level: usize,
    pub name: &'a str,
    pub(crate) meta: MetaData,
    body: FdtReader<'a, 'b>,
}

impl<'a, 'b: 'a> Node<'a, 'b> {
    pub(crate) fn new(level: usize, reader: &mut FdtReader<'a, 'b>) -> Self {
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

    pub fn propertys(&self) -> impl Iterator<Item = Property<'a, 'b>> + '_ {
        let reader = self.body.clone();
        PropIter { reader }
    }
}

struct PropIter<'a, 'b: 'a> {
    reader: FdtReader<'a, 'b>,
}

impl<'a, 'b: 'a> Iterator for PropIter<'a, 'b> {
    type Item = Property<'a, 'b>;

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
