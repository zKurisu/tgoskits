pub use fdt_parser::*;

use crate::driver::device_tree::get_device_tree;

pub fn device_tree() -> Option<Fdt> {
    get_device_tree()
}
