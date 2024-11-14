use core::{alloc::Layout, ptr::NonNull};

use super::{Virt, VirtToPhys};

/// Allocates **coherent** memory that meets Direct Memory Access (DMA) requirements.
///
/// This function allocates a block of memory through the global allocator. The memory pages must be contiguous, undivided, and have consistent read and write access.
///
/// - `layout`: The memory layout, which describes the size and alignment requirements of the requested memory.
///
/// Returns an [`DMAInfo`] structure containing details about the allocated memory, such as the starting address and size. If it's not possible to allocate memory meeting the criteria, returns [`None`].
/// # Safety
/// This function is unsafe because it directly interacts with the global allocator, which can potentially cause memory leaks or other issues if not used correctly.
pub unsafe fn alloc_coherent(layout: Layout) -> Option<DMAMem> {
    let cpu_addr = NonNull::new(alloc::alloc::alloc(layout))?;
    let bus_addr = Virt::<u8>::from(cpu_addr.as_ptr() as usize);
    let bus_addr = bus_addr.to_phys();
    let bus_addr: usize = bus_addr.into();

    Some(DMAMem {
        cpu_addr,
        bus_addr: BusAddr(bus_addr as _),
    })
}

/// Frees coherent memory previously allocated.
///
/// This function releases the memory block that was previously allocated and marked as coherent. It ensures proper deallocation and management of resources associated with the memory block.
///
/// - `dma_info`: An instance of [`DMAInfo`] containing the details of the memory block to be freed, such as its starting address and size.
/// # Safety
/// This function is unsafe because it directly interacts with the global allocator, which can potentially cause memory leaks or other issues if not used correctly.
pub unsafe fn dealloc_coherent(dma: DMAMem, layout: Layout) {
    alloc::alloc::dealloc(dma.cpu_addr.as_ptr(), layout);
}

/// Represents information related to a DMA operation.
#[derive(Debug, Clone, Copy)]
pub struct DMAMem {
    /// The `cpu_addr` field represents the address at which the CPU accesses this memory region.
    /// This address is a virtual memory address used by the CPU to access memory.
    pub cpu_addr: NonNull<u8>,
    /// The `bus_addr` field represents the physical address of this memory region on the bus.
    /// The DMA controller uses this address to directly access memory.
    pub bus_addr: BusAddr,
}

/// A bus memory address.
///
/// It's a wrapper type around an [`u64`].
#[repr(transparent)]
#[derive(Copy, Clone, Default, Ord, PartialOrd, Eq, PartialEq)]
pub struct BusAddr(u64);

impl BusAddr {
    /// Converts an [`u64`] to a physical address.
    pub const fn new(addr: u64) -> Self {
        Self(addr)
    }

    /// Converts the address to an [`u64`].
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

impl From<u64> for BusAddr {
    fn from(value: u64) -> Self {
        Self::new(value)
    }
}

impl core::fmt::Debug for BusAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("BusAddr")
            .field(&format_args!("{:#X}", self.0))
            .finish()
    }
}
