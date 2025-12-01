use core::time::Duration;

pub use heapless::Vec as StackVec;
use kernutil::define_ids;
pub use kernutil::memory::MemoryDescriptor;

#[trait_ffi::def_extern_trait(mod_path = "hal::al")]
pub trait Memory {
    /// Convert virtual address to physical address
    /// # Safety
    /// The caller must ensure that the provided virtual address is valid and mapped.
    unsafe fn virt_to_phys(virt: *mut u8) -> usize;
    fn phys_to_virt(phys: usize) -> *mut u8;
    fn page_size() -> usize;
    fn memory_map() -> StackVec<MemoryDescriptor, 64>;

    // fn page_table_new_base() -> PageTabeAddr;
    // fn page_table_drop(addr: PageTabeAddr);
    // fn page_table_clone(addr: PageTabeAddr) -> PageTabeAddr;

    // fn kernel_page_table() -> PageTabeAddr;
    // fn set_kernel_page_table(addr: PageTabeAddr);
}

#[trait_ffi::def_extern_trait(not_def_impl, mod_path = "hal::al")]
pub trait Platform {
    fn post_allocator();
    fn irq_is_enabled(irq: IrqId) -> bool;
    fn irq_set_enabled(irq: IrqId, enabled: bool);
    fn shutdown() -> !;
}

#[trait_ffi::def_extern_trait(not_def_impl, mod_path = "hal::al")]
pub trait Cpu {
    fn current_cpu_id() -> usize;
    fn irq_local_is_enabled() -> bool;
    fn irq_local_set_enable(enabled: bool);
    fn systimer_irq() -> IrqId;
    fn systimer_enable();
    fn systimer_disable();
    fn systimer_set_next_event(intval: Duration);
    fn systimer_ack();
    fn systimer_since_boot() -> Duration;
}

#[trait_ffi::def_extern_trait(mod_path = "hal::al", not_def_impl)]
pub trait Console {
    fn early_write(bytes: &[u8]) -> usize;
    fn early_read() -> Option<u8>;
}

pub fn handle_irq(irq: IrqId) {
    crate::os::irq::handle_irq(irq);
}

define_ids! {
    PageTabeAddr(usize),
    IrqId(usize),
}
