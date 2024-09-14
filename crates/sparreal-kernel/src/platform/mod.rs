use core::{fmt, ptr::NonNull};

pub use page_table_interface::PageTableFn;

pub trait Platform: Mmu + Sync + Send {
    fn wait_for_interrupt();

    fn current_ticks() -> u64;

    fn tick_hz() -> u64;
}

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

#[cfg(not(feature = "mmu"))]
pub trait Mmu {}

#[cfg(feature = "mmu")]
pub trait Mmu {
    type Table: PageTableFn;

    fn set_kernel_page_table(table: &Self::Table);
    fn set_user_page_table(table: Option<&Self::Table>);
    fn get_kernel_page_table() -> Self::Table;
    fn flush_tlb(addr: Option<NonNull<u8>>);
    fn boot_debug_writer() -> Option<impl fmt::Write> {
        None::<NopWrite>
    }
}

struct NopWrite;
impl fmt::Write for NopWrite {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        todo!()
    }
}

pub fn app_main() {
    extern "C" {
        fn __sparreal_rt_main();
    }

    unsafe { __sparreal_rt_main() }
}
