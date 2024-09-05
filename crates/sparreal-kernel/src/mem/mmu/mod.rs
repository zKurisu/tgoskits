use core::ptr::NonNull;

use flat_device_tree::Fdt;
pub use page_table_interface::*;

pub unsafe fn boot_init<T: PageTable>(va_offset: usize, dtb_addr: NonNull<u8>){
    let fdt = Fdt::from_ptr(dtb_addr.as_ptr()).unwrap();
    

}
