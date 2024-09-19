use core::{fmt, ptr::NonNull, time::Duration};

use buddy_system_allocator::Heap;
use memory_addr::PhysAddr;
pub use page_table_interface::PageTableFn;
use page_table_interface::{Access, MapConfig, PagingResult};
use sparreal_macros::api_trait;

use crate::mem::{PageAllocator, Phys, Virt};

pub trait Platform: Mmu + Sync + Send {
    fn wait_for_interrupt();

    fn current_ticks() -> u64;

    fn tick_hz() -> u64;

    fn since_boot() -> Duration {
        let current_tick = Self::current_ticks();
        let freq = Self::tick_hz();
        Duration::from_nanos(current_tick * 1_000_000_000 / freq)
    }
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

#[api_trait]
pub trait Platform2 {
    unsafe fn current_ticks() -> u64;
    unsafe fn tick_hz() -> u64;
    unsafe fn debug_write_char(ch: char);
    unsafe fn table_new(access: &mut PageAllocator) -> PagingResult<Phys<u8>>;
    unsafe fn table_map(
        table: Phys<u8>,
        config: MapConfig,
        size: usize,
        allow_block: bool,
        access: &mut PageAllocator,
    ) -> PagingResult<()>;

    unsafe fn set_kernel_page_table(table: Phys<u8>);
    unsafe fn set_user_page_table(table: Option<Phys<u8>>);
    unsafe fn get_kernel_page_table() -> Phys<u8>;
    unsafe fn flush_tlb(addr: Option<Virt<u8>>);
}
