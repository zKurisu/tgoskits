use core::{ptr::NonNull, time::Duration};

pub use heapless::Vec as StackVec;
use kernutil::define_type;
pub use kernutil::memory::MemoryDescriptor;

#[trait_ffi::def_extern_trait(mod_path = "hal::al")]
pub trait Memory {
    /// Convert virtual address to physical address
    fn virt_to_phys(virt: VirtAddr) -> PhysAddr;
    fn phys_to_virt(phys: PhysAddr) -> VirtAddr;
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

#[derive(thiserror::Error, Debug)]
pub enum PageError {
    #[error("Invalid Address")]
    InvalidAddress,
    #[error("Out of Memory")]
    OutOfMemory,
    #[error("Page Already Exists")]
    Exist,
}

#[derive(Clone, Copy, Debug)]
pub struct MapSettings {
    pub access: AccessFlags,
    pub mem_attributes: MemAttributes,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct AccessFlags: usize {
        const READ = 1;
        const WRITE = 1<<2;
        const EXECUTE = 1<<3;
        const LOWER = 1<<4;
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct MemAttributes: usize {
        const NORMAL = 0;
        const DEVICE = 1<<0;
        const UNCACHED = 1<<1;
    }
}

pub trait PageTable {
    fn addr(&self) -> PhysAddr;
    fn map(
        &mut self,
        phys_start: PhysAddr,
        virt_start: VirtAddr,
        size: usize,
        settings: MapSettings,
    ) -> Result<(), PageError>;
    fn unmap(&mut self, virt_start: VirtAddr, size: usize) -> Result<(), PageError>;
}

define_type! {
    /// Interrupt Request Identifier
    IrqId(usize),
    /// Physical Address
    PhysAddr(usize),
    /// Virtual Address
    VirtAddr(usize),
}
