pub use fdt_parser::*;

use crate::driver::device_tree::get_device_tree;

pub fn device_tree<'a>() -> Option<Fdt<'a>> {
    get_device_tree()
}
