use page_table_generic::{PTEArch, PTEGeneric};

use crate::platform::{self, new_pte, page_size, read_pte, table_level};

pub type PageTableRef<'a> = page_table_generic::PageTableRef<'a, PTEImpl>;

pub fn get_kernal_table<'a>() -> PageTableRef<'a> {
    let addr = unsafe { platform::get_kernel_table() };
    let level = unsafe { platform::table_level() };
    PageTableRef::from_addr(addr, level)
}

#[derive(Clone, Copy)]
pub struct PTEImpl;

impl PTEArch for PTEImpl {
    fn page_size() -> usize {
        unsafe { page_size() }
    }

    fn level() -> usize {
        unsafe { table_level() }
    }

    fn new_pte(config: PTEGeneric) -> usize {
        unsafe { new_pte(config) }
    }

    fn read_pte(pte: usize) -> PTEGeneric {
        unsafe { read_pte(pte) }
    }
}
