use alloc::boxed::Box;
use byte_unit::{Byte, UnitType};
use kernutil::memory::MemoryType;

use crate::hal::al::*;

pub fn init() {
    info!("Setting up MMU and page tables");

    let mut pt = memory::page_table_new();
    map_regions(&mut pt);
    let pt_addr = pt.addr();
    debug!("Setting kernel page table to {pt_addr:?}");
    memory::set_kernel_page_table(pt_addr);
    memory::enable_paging();
}

fn map_regions(pt: &mut Box<dyn PageTable>) {
    for region in memory::memory_map() {
        let phys = PhysAddr::from(region.physical_start);
        let virt = VirtAddr::from(phys);
        let fmt = Byte::from(region.size_in_bytes).get_appropriate_unit(UnitType::Binary);
        let config = match region.memory_type {
            MemoryType::Mmio => MemConfig {
                access: AccessFlags::READ | AccessFlags::WRITE,
                attrs: MemAttributes::Device,
            },
            _ => MemConfig {
                access: AccessFlags::READ | AccessFlags::WRITE | AccessFlags::EXECUTE,
                attrs: MemAttributes::Normal,
            },
        };

        debug!(
            "Mapping `{:<16}`: [{:>#016x}, {:>#016x}) -> [{:>#016x}, {:>#016x}) {} ({:#.2})",
            region.name,
            virt.raw(),
            (virt.raw() + region.size_in_bytes),
            phys.raw(),
            (phys.raw() + region.size_in_bytes),
            config,
            fmt
        );
        pt.map(
            virt.raw().into(),
            phys.raw().into(),
            region.size_in_bytes,
            config,
            false,
        )
        .expect("Failed to map memory region");
    }
}
