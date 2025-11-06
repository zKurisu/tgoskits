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

    println!("\n=== 映前状态 - walk_all (遍历所有项) ===");
    for p in pg.walk(WalkConfig {
        start_vaddr: VirtAddr::new(0),
        end_vaddr: VirtAddr::new(usize::MAX),
    }) {
        println!(
            "l: {}, va: {:?}, pte: {:?}, final: {}",
            p.level, p.vaddr, p.pte, p.is_final_mapping
        );
    }

    pg.map(&MapConfig {
        vaddr: 0x0000f00000000000usize.into(), // 使用期望输出的地址
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
    println!("walk_valid() 返回 {count} 个叶子级别条目");

    println!("\n=== 映后状态 - 显示完整层次（所有有效项） ===");
    for p in pg.walk(WalkConfig {
        start_vaddr: VirtAddr::new(0),
        end_vaddr: VirtAddr::new(usize::MAX),
    }) {
        println!(
            "l: {}, va: {:?}, c: PTE PA: {:?} Block: {}, Final: {}",
            p.level,
            p.vaddr,
            p.pte.paddr(),
            p.pte.is_huge(),
            p.is_final_mapping
        );
    }

    assert_eq!(count, 2); // walk_valid() 应该返回2个叶子级别条目
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
fn test_walk_all_entries() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Trace)
        .try_init();

    let mut pg = PageTable::<T4kL4, Fram4k>::new(Fram4k).unwrap();

    // 映射一个低地址范围，便于验证
    pg.map(&MapConfig {
        vaddr: 0x1000usize.into(), // 使用简单地址
        paddr: 0x0000usize.into(),
        size: 0x2000,
        pte: PteImpl(0),
        allow_huge: false,
        flush: false,
    })
    .unwrap();

    println!("\n=== walker遍历所有页表项 ===");
    let mut count_all = 0;
    let mut count_final = 0;
    for p in pg.walk(WalkConfig {
        start_vaddr: VirtAddr::new(0),
        end_vaddr: VirtAddr::new(usize::MAX),
    }) {
        println!(
            "l: {}, va: {:?}, c: PTE PA: {:?} Block: {} | Final: {} Valid: {}",
            p.level,
            p.vaddr,
            p.pte.paddr(),
            p.pte.is_huge(),
            p.is_final_mapping,
            p.pte.valid()
        );
        count_all += 1;
        if p.is_final_mapping {
            count_final += 1;
        }
    }
    println!("共返回 {count_all} 个页表项，其中 {count_final} 个是最终映射");

    println!("\n=== 使用walk_valid()过滤仅最终映射 ===");
    let mut count = 0;
    for p in pg.walk_valid() {
        println!(
            "l: {}, va: {:?}, c: PTE PA: {:?} Block: {} | Final: {}",
            p.level,
            p.vaddr,
            p.pte.paddr(),
            p.pte.is_huge(),
            p.is_final_mapping
        );
        count += 1;
    }
    println!("walk_valid() 共返回 {count} 个最终映射页表项");
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

    // 测试有效叶子项（模拟）
    let mut leaf_pte = PteImpl(0);
    leaf_pte.set_valid(true);
    leaf_pte.set_paddr(0x1000usize.into());
    println!("\n叶子项:");
    println!("  valid: {}", leaf_pte.valid());
    println!("  is_huge: {}", leaf_pte.is_huge());

    // 测试大页项
    let mut huge_pte = PteImpl(0);
    huge_pte.set_valid(true);
    huge_pte.set_paddr(0x200000usize.into());
    huge_pte.set_is_huge(true);
    println!("\n大页项:");
    println!("  valid: {}", huge_pte.valid());
    println!("  is_huge: {}", huge_pte.is_huge());

    // 验证基本断言
    assert!(!invalid_pte.valid());
    assert!(leaf_pte.valid());
    assert!(!leaf_pte.is_huge());
    assert!(huge_pte.valid());
    assert!(huge_pte.is_huge());
}
