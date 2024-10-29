use alloc::string::String;
use page_table_interface::{MapConfig, PagingResult};
use sparreal_macros::api_trait;

use crate::mem::{PageAllocatorRef, Phys, Virt};

pub enum PageAttribute {
    Read,
    Write,
    Device,
    Execute,
    NonCache,
}

pub enum PageError {
    NoMemory,
    Other,
}

pub fn app_main() {
    extern "C" {
        fn __sparreal_rt_main();
    }

    unsafe { __sparreal_rt_main() }
}

#[api_trait]
pub trait Platform {
    unsafe fn shutdown();
    unsafe fn wait_for_interrupt();
    unsafe fn current_ticks() -> u64;
    unsafe fn tick_hz() -> u64;
    unsafe fn debug_write_char(ch: u8);
    unsafe fn table_new(access: &mut PageAllocatorRef) -> PagingResult<Phys<u8>>;
    unsafe fn table_map(
        table: Phys<u8>,
        config: MapConfig,
        size: usize,
        allow_block: bool,
        flush: bool,
        access: &mut PageAllocatorRef,
    ) -> PagingResult<()>;

    unsafe fn set_kernel_page_table(table: Phys<u8>);
    unsafe fn set_user_page_table(table: Option<Phys<u8>>);
    unsafe fn get_kernel_page_table() -> Phys<u8>;
    unsafe fn flush_tlb(addr: Option<Virt<u8>>);
    fn print_system_info();

    fn irqs_enable();
    fn irqs_disable();
    fn cpu_id() -> u64;
    fn cpu_id_display() -> String;
}
