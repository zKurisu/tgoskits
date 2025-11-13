#[macro_use]
mod _macros;

mod addrspace;
pub(crate) mod entry;
mod head;
mod register;
mod relocate;

pub use relocate::relocate;

use crate::ArchTrait;

pub struct Arch;

impl ArchTrait for Arch {
    fn kernel_code() -> &'static [u8] {
        let start = ext_sym_addr!(_head);
        let end = ext_sym_addr!(__kernel_code_end);
        unsafe { core::slice::from_raw_parts(start as *const u8, end - start) }
    }

    fn post_allocator() {}
}
