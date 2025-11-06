//! Mock implementations for testing
//!
//! This module provides mock implementations used in tests for the page-table-generic crate.
#![cfg(not(target_os = "none"))]

use page_table_generic::*;

mod mocks;

use mocks::*;

#[test]
fn test_pte() {
    let mut want = PteImpl(0);
    want.set_valid(true);
    assert!(want.valid());

    let addr = PhysAddr::from(0xff123456000usize);
    want.set_paddr(addr);
    assert_eq!(want.paddr(), addr);
}

fn test_high<T: TableGeneric, A: FramAllocator>(pte: T::P, alloc: A)
where
    T::P: std::fmt::Debug,
{
    let mut pg = PageTable::<T, A>::new(alloc).unwrap();

    println!("\n=== 映前状态 - walk_all (包括无效) ===");
    for p in pg.walk(WalkConfig {
        start_vaddr: VirtAddr::new(0),
        end_vaddr: VirtAddr::new(core::usize::MAX),
        visit_invalid: true,
        visit_indirect: false,
    }) {
        println!("l: {}, va: {:?}, pte: {:?}", p.level, p.vaddr, p.pte);
    }

    pg.map(&MapConfig {
        vaddr: 0x0000f00000000000usize.into(),  // 使用期望输出的地址
        paddr: 0x0000usize.into(),
        size: 0x2000,
        pte,
        allow_huge: false,
        flush: false,
    })
    .unwrap();

    println!("\n=== 映后状态 - walk_valid结果 ===");
    let mut count = 0;
    for p in pg.walk_valid() {
        println!("l: {}, va: {:?}, pte: {:?}", p.level, p.vaddr, p.pte);
        count += 1;
    }

    // 注意：walk_valid()只返回叶子级别的有效条目，所以是2个
    // 我们期望的5个条目来自自定义walker，包括中间级别
    println!("walk_valid() 返回 {} 个叶子级别条目", count);

    println!("\n=== 映后状态 - 使用visit_indirect=true显示完整层次 ===");
    for p in pg.walk(WalkConfig {
        start_vaddr: VirtAddr::new(0),
        end_vaddr: VirtAddr::new(core::usize::MAX),
        visit_invalid: false,
        visit_indirect: true,  // 显示所有级别的有效页表项
    }) {
        // 只显示目标地址附近的条目
        if p.vaddr.raw() >= 0x0000f00000000000 - 0x1000_0000 && p.vaddr.raw() <= 0x0000f00000000000 + 0x1000_0000 {
            println!("l: {}, va: {:?}, c: PTE PA: {:?} Block: {}",
                     p.level, p.vaddr, p.pte.paddr(), p.pte.is_huge());
        }
    }

    assert_eq!(count, 2);  // walk_valid() 应该返回2个叶子级别条目
}


#[test]
fn test_new() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Trace)
        .try_init();

    test_high::<T4kL4, Fram4k>(PteImpl(0), Fram4k);
}

#[test]
fn test_walk_with_visit_indirect() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Trace)
        .try_init();

    let mut pg = PageTable::<T4kL4, Fram4k>::new(Fram4k).unwrap();

    // 映射一个低地址范围，便于验证
    pg.map(&MapConfig {
        vaddr: 0x1000usize.into(),  // 使用简单地址
        paddr: 0x0000usize.into(),
        size: 0x2000,
        pte: PteImpl(0),
        allow_huge: false,
        flush: false,
    }).unwrap();

    println!("\n=== 使用visit_indirect=true的walker遍历完整层次 ===");
    let mut count = 0;
    for p in pg.walk(WalkConfig {
        start_vaddr: VirtAddr::new(0),
        end_vaddr: VirtAddr::new(core::usize::MAX),
        visit_invalid: false,
        visit_indirect: true,  // 启用访问间接页表项
    }) {
        println!("l: {}, va: {:?}, c: PTE PA: {:?} Block: {} | Leaf: {} Indirect: {} Final: {}",
                 p.level, p.vaddr, p.pte.paddr(), p.pte.is_huge(),
                 p.pte.is_leaf_mapping(p.level),
                 p.pte.is_indirect_entry(p.level),
                 p.pte.is_final_mapping());
        count += 1;
    }
    println!("visit_indirect=true 共返回 {} 个页表项", count);

    println!("\n=== 使用visit_indirect=false的walker对比（仅叶子级别） ===");
    let mut count = 0;
    for p in pg.walk(WalkConfig {
        start_vaddr: VirtAddr::new(0),
        end_vaddr: VirtAddr::new(core::usize::MAX),
        visit_invalid: false,
        visit_indirect: false,  // 仅叶子级别
    }) {
        println!("l: {}, va: {:?}, c: PTE PA: {:?} Block: {} | Leaf: {} Indirect: {} Final: {}",
                 p.level, p.vaddr, p.pte.paddr(), p.pte.is_huge(),
                 p.pte.is_leaf_mapping(p.level),
                 p.pte.is_indirect_entry(p.level),
                 p.pte.is_final_mapping());
        count += 1;
    }
    println!("visit_indirect=false 共返回 {} 个页表项", count);
}

#[test]
fn test_page_table_entry_methods() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Trace)
        .try_init();

    // 测试无效项
    let invalid_pte = PteImpl(0);
    println!("无效项:");
    println!("  valid: {}", invalid_pte.valid());
    println!("  is_leaf_mapping(1): {}", invalid_pte.is_leaf_mapping(1));
    println!("  is_indirect_entry(4): {}", invalid_pte.is_indirect_entry(4));
    println!("  is_final_mapping: {}", invalid_pte.is_final_mapping());

    // 测试有效叶子项（模拟）
    let mut leaf_pte = PteImpl(0);
    leaf_pte.set_valid(true);
    leaf_pte.set_paddr(0x1000usize.into());
    println!("\n叶子项:");
    println!("  valid: {}", leaf_pte.valid());
    println!("  is_huge: {}", leaf_pte.is_huge());
    println!("  is_leaf_mapping(1): {}", leaf_pte.is_leaf_mapping(1));
    println!("  is_leaf_mapping(4): {}", leaf_pte.is_leaf_mapping(4));
    println!("  is_indirect_entry(1): {}", leaf_pte.is_indirect_entry(1));
    println!("  is_indirect_entry(4): {}", leaf_pte.is_indirect_entry(4));
    println!("  is_final_mapping: {}", leaf_pte.is_final_mapping());

    // 测试大页项
    let mut huge_pte = PteImpl(0);
    huge_pte.set_valid(true);
    huge_pte.set_paddr(0x200000usize.into());
    huge_pte.set_is_huge(true);
    println!("\n大页项:");
    println!("  valid: {}", huge_pte.valid());
    println!("  is_huge: {}", huge_pte.is_huge());
    println!("  is_leaf_mapping(1): {}", huge_pte.is_leaf_mapping(1));
    println!("  is_indirect_entry(4): {}", huge_pte.is_indirect_entry(4));
    println!("  is_final_mapping: {}", huge_pte.is_final_mapping());

    // 测试entry_type方法
    println!("\n页表项类型:");
    println!("  无效项类型: {}", invalid_pte.entry_type());
    println!("  叶子项类型: {}", leaf_pte.entry_type());
    println!("  大页项类型: {}", huge_pte.entry_type());

    // 验证断言
    assert!(!invalid_pte.is_final_mapping());
    assert!(leaf_pte.is_leaf_mapping(1));
    assert!(!leaf_pte.is_leaf_mapping(4));
    assert!(!leaf_pte.is_indirect_entry(1));
    assert!(leaf_pte.is_indirect_entry(4));
    assert!(leaf_pte.is_final_mapping());
    assert!(huge_pte.is_final_mapping());
    assert!(!huge_pte.is_leaf_mapping(1)); // 大页不是叶子项
    assert_eq!(invalid_pte.entry_type(), "Invalid");
    assert_eq!(leaf_pte.entry_type(), "Leaf");
    assert_eq!(huge_pte.entry_type(), "HugePage");
}
