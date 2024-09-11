#![no_std]

mod common;

#[cfg_attr(any(test, target_arch = "aarch64"), path = "arch/aarch64/mod.rs")]
mod arch;

pub use arch::*;

// #[cfg_attr("test", path = "arch/aarch64/mod.rs")]
// pub mod arch;

#[cfg(test)]
mod test {
    extern crate std;

    use arch::PTE;
    use log::{info, LevelFilter};
    use page_table_interface::*;

    use super::*;

    fn init() {
        let _ = env_logger::builder()
            .filter_level(LevelFilter::Trace)
            .is_test(true)
            .try_init();
    }

    struct AcImpl;

    impl Access for AcImpl {
        unsafe fn alloc(&mut self, layout: core::alloc::Layout) -> Option<memory_addr::PhysAddr> {
            Some((std::alloc::alloc(layout) as usize).into())
        }

        unsafe fn dealloc(&mut self, ptr: memory_addr::PhysAddr, layout: core::alloc::Layout) {
            std::alloc::dealloc(ptr.as_usize() as _, layout)
        }

        fn va_offset(&self) -> usize {
            0
        }
    }

    #[test]
    fn test_l1() {
        unsafe {
            let mut access = AcImpl;
            let mut table = PageTableRef::<'_, PTE, 512, 4>::new(4, &mut access).unwrap();
            let vaddr = (0xffff_ffff_0000_0000 + 50 * 0x1000).into();
            let paddr = 0x1000.into();

            table
                .map(
                    &MapConfig {
                        vaddr,
                        paddr,
                        attrs: PageAttribute::Read | PageAttribute::Write,
                    },
                    1,
                    &mut access,
                )
                .unwrap();

            let pte = table.get_pte_mut(vaddr, &mut access);

            assert!(pte.is_some());

            assert!(pte.unwrap().paddr() == paddr)
        }
    }
    #[test]
    fn test_l2() {
        unsafe {
            let mut access = AcImpl;
            let mut table = PageTableRef::<'_, PTE, 512, 4>::new(4, &mut access).unwrap();
            let vaddr = (0xffff_ffff_0000_0000 + 50 * 2 * 1024 * 1024).into();
            let paddr = 0x1000.into();

            table
                .map(
                    &MapConfig {
                        vaddr,
                        paddr,
                        attrs: PageAttribute::Read | PageAttribute::Write,
                    },
                    2,
                    &mut access,
                )
                .unwrap();

            let pte = table.get_pte_mut(vaddr, &mut access);

            assert!(pte.is_some());

            assert!(pte.unwrap().paddr() == paddr)
        }
    }

    #[test]
    fn test_l3() {
        unsafe {
            let mut access = AcImpl;
            let mut table = PageTableRef::<'_, PTE, 512, 4>::new(4, &mut access).unwrap();
            let vaddr = (0xffff_ff00_0000_0000 + 50 * 1024 * 1024 * 1024).into();
            let paddr = 0x1000.into();

            table
                .map(
                    &MapConfig {
                        vaddr,
                        paddr,
                        attrs: PageAttribute::Read | PageAttribute::Write,
                    },
                    3,
                    &mut access,
                )
                .unwrap();

            let pte = table.get_pte_mut(vaddr, &mut access);

            assert!(pte.is_some());

            assert!(pte.unwrap().paddr() == paddr)
        }
    }

    #[test]
    fn test_table() {
        init();
        unsafe {
            let mut access = AcImpl;

            let table_l1 = PageTableRef::<'_, PTE, 512, 4>::new(1, &mut access).unwrap();
            info!("L1 entry_size = {:#X}", table_l1.entry_size());

            let mut table = PageTableRef::<'_, PTE, 512, 4>::new(4, &mut access).unwrap();

            let virt = 0xffff_ffff_0000_0000 + 1024 * 1024 * 1024;
            let phys = 0x1000;

            table
                .map(
                    &MapConfig {
                        vaddr: virt.into(),
                        paddr: phys.into(),
                        attrs: PageAttribute::Read | PageAttribute::Write,
                    },
                    2,
                    &mut access,
                )
                .unwrap();

            info!("created table");

            table.walk(
                |info| {
                    info!("L{} {:#X} {:?}", info.level, info.vaddr, info.pte);
                },
                &access,
            );

            info!("walk finish");

            let pte = table.get_pte_mut(virt.into(), &mut access);

            info!("pte: {:?}", pte);

            assert!(pte.is_some());

            assert!(pte.unwrap().paddr() == phys.into())
        }
    }

    const BYTES_1G: usize = 1024 * 1024 * 1024;

    #[test]
    fn test_table_map_region() {
        init();
        unsafe {
            let mut access = AcImpl;
            let mut table = PageTableRef::<'_, PTE, 512, 4>::new(4, &mut access).unwrap();
            let va_offset = 0xffff_ff00_0000_0000;
            let kernel = 0x4008_0000;

            let kernel_p = VirtAddr::from(kernel);
            let vphys_down = kernel_p.align_down(BYTES_1G);
            let phys_down = PhysAddr::from(vphys_down.as_usize());
            let virt_down = vphys_down + va_offset;

            table
                .map_region(
                    MapConfig {
                        vaddr: vphys_down,
                        paddr: phys_down,
                        attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
                    },
                    BYTES_1G,
                    true,
                    &mut access,
                    &|_| {},
                )
                .unwrap();

            table
                .map_region(
                    MapConfig {
                        vaddr: virt_down,
                        paddr: phys_down,
                        attrs: PageAttribute::Read | PageAttribute::Write | PageAttribute::Execute,
                    },
                    BYTES_1G,
                    true,
                    &mut access,
                    &|_| {},
                )
                .unwrap();

            info!("created table");

            table.walk(
                |info| {
                    info!("L{} {:#X} {:?}", info.level, info.vaddr, info.entry);
                },
                &access,
            );

            info!("walk finish");
        }
    }
}
