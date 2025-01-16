pub use driver_interface::DriverRegisterListRef;
use page_table_generic::PTEGeneric;
use sparreal_macros::api_trait;

use crate::mem::KernelRegions;

#[api_trait]
pub trait Platform {
    fn kernel_regions() -> KernelRegions;
    fn kstack_size() -> usize;
    fn cpu_id() -> usize;

    fn wait_for_interrupt();
    fn on_boot_success() {}
    fn shutdown() -> !;
    fn debug_put(b: u8);
    fn current_ticks() -> u64;
    fn tick_hz() -> u64;
    fn driver_registers() -> DriverRegisterListRef;
}

#[cfg(feature = "mmu")]
#[api_trait]
pub trait MMU {
    fn set_kernel_table(addr: usize);
    fn get_kernel_table() -> usize;
    fn set_user_table(addr: usize);
    fn get_user_table() -> usize;
    fn flush_tlb(addr: *const u8);
    fn flush_tlb_all();
    fn page_size() -> usize;
    fn table_level() -> usize;
    fn new_pte(config: PTEGeneric) -> usize;
    fn read_pte(pte: usize) -> PTEGeneric;
    fn enable_mmu(stack_top: usize, jump_to: usize) -> !;
}
