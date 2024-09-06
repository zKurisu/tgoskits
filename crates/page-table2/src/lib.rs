#![no_std]

mod common;

#[cfg_attr(any(test, target_arch = "aarch64"), path = "arch/aarch64/mod.rs")]
mod arch;

// #[cfg_attr("test", path = "arch/aarch64/mod.rs")]
// pub mod arch;

#[cfg(test)]
mod test {
    extern crate std;

    use arch::PTE;
    use log::{info, LevelFilter};
    use page_table_interface::{
        Access, GenericPTE, MapConfig, PageAttribute, PageTableMap, PageTableRef,
    };

    use super::*;

    fn init() {
        let _ = env_logger::builder()
            .filter_level(LevelFilter::Trace)
            .is_test(true)
            .try_init();
    }

    struct AcImpl;

    impl Access for AcImpl {
        const VA_OFFSET: usize = 0;

        unsafe fn alloc(&mut self, layout: core::alloc::Layout) -> Option<memory_addr::PhysAddr> {
            Some((std::alloc::alloc(layout) as usize).into())
        }

        unsafe fn dealloc(&mut self, ptr: memory_addr::PhysAddr, layout: core::alloc::Layout) {
            std::alloc::dealloc(ptr.as_usize() as _, layout)
        }
    }

    #[test]
    fn test_l1() {
        unsafe {
            let mut access = AcImpl;
            let mut table = PageTableRef::<'_, PTE, 512, _>::new(4, &mut access).unwrap();
            let vaddr = (0xffff_ffff_0000_0000 + 50 * 0x1000).into();
            let paddr = 0x1000.into();

            table
                .map(&MapConfig {
                    vaddr,
                    paddr,
                    page_level: 1,
                    attrs: PageAttribute::Read | PageAttribute::Write,
                })
                .unwrap();

            let pte = table.get_pte_mut(vaddr);

            assert!(pte.is_some());

            assert!(pte.unwrap().paddr() == paddr)
        }
    }
    #[test]
    fn test_l2() {
        unsafe {
            let mut access = AcImpl;
            let mut table = PageTableRef::<'_, PTE, 512, _>::new(4, &mut access).unwrap();
            let vaddr = (0xffff_ffff_0000_0000 + 50 * 2 * 1024 * 1024).into();
            let paddr = 0x1000.into();

            table
                .map(&MapConfig {
                    vaddr,
                    paddr,
                    page_level: 2,
                    attrs: PageAttribute::Read | PageAttribute::Write,
                })
                .unwrap();

            let pte = table.get_pte_mut(vaddr);

            assert!(pte.is_some());

            assert!(pte.unwrap().paddr() == paddr)
        }
    }

    #[test]
    fn test_l3() {
        unsafe {
            let mut access = AcImpl;
            let mut table = PageTableRef::<'_, PTE, 512, _>::new(4, &mut access).unwrap();
            let vaddr = (0xffff_ff00_0000_0000 + 50 * 1024 * 1024 * 1024).into();
            let paddr = 0x1000.into();

            table
                .map(&MapConfig {
                    vaddr,
                    paddr,
                    page_level: 3,
                    attrs: PageAttribute::Read | PageAttribute::Write,
                })
                .unwrap();

            let pte = table.get_pte_mut(vaddr);

            assert!(pte.is_some());

            assert!(pte.unwrap().paddr() == paddr)
        }
    }

    #[test]
    fn test_table() {
        init();
        unsafe {
            let mut access = AcImpl;

            let table_l1 = PageTableRef::<'_, PTE, 512, _>::new(1, &mut access).unwrap();
            info!("L1 entry_size = {:#X}", table_l1.entry_size());

            let mut table = PageTableRef::<'_, PTE, 512, _>::new(4, &mut access).unwrap();

            let virt = 0xffff_ffff_0000_0000 + 1024 * 1024 * 1024;
            let phys = 0x1000;

            table.map(&MapConfig {
                vaddr: virt.into(),
                paddr: phys.into(),
                page_level: 2,
                attrs: PageAttribute::Read | PageAttribute::Write,
            });

            info!("created table");

            table.walk(|info| {
                info!("L{} {:#X} {:?}", info.level, info.vaddr, info.pte);
            });

            info!("walk finish");

            let pte = table.get_pte_mut(virt.into());

            info!("pte: {:?}", pte);

            assert!(pte.is_some());

            assert!(pte.unwrap().paddr() == phys.into())
        }
    }
}
