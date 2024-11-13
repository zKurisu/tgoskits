use alloc::string::String;
use page_table_generic::PTEGeneric;
use sparreal_macros::api_trait;

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

    fn print_system_info();

    fn irqs_enable();
    fn irqs_disable();
    fn cpu_id() -> u64;
    fn cpu_id_display() -> String;
}

#[api_trait]
pub trait PlatformPageTable {
    fn set_kernel_table(addr: usize);
    fn get_kernel_table() -> usize;
    fn set_user_table(addr: usize);
    fn get_user_table() -> usize;
    fn flush_tlb(addr: Option<*const u8>);
    fn page_size() -> usize;
    fn table_level() -> usize;
    fn new_pte(config: PTEGeneric) -> usize;
    fn read_pte(pte: usize) -> PTEGeneric;
}
