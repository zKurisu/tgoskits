use page_table_generic::*;
mod mocks;
use mocks::*;

// ===== 页表标志位测试 =====

#[test]
fn test_pte() {
    let pte = PteImpl::new();
    println!("PTE: {:?}", pte);
    assert!(!pte.valid());
    assert!(!pte.is_huge());
    println!("✓ Empty PTE test passed");
}

#[test]
fn test_pte_read_only() {
    let pte = PteImpl::read_only();
    assert!(pte.valid());
    assert!(pte.is_readable());
    assert!(!pte.is_writable());
    assert!(!pte.is_user_executable());
    assert!(!pte.is_user_accessible());
    assert!(!pte.is_privilege_executable());
    assert_eq!(pte.cache_mode(), 1); // normal cache
    assert!(!pte.is_huge());
    println!("✓ ReadOnly PTE test passed");
}

#[test]
fn test_pte_user_mode() {
    let pte = PteImpl::user_mode();
    assert!(pte.valid());
    assert!(pte.is_readable());
    assert!(pte.is_writable());
    assert!(pte.is_user_executable());
    assert!(pte.is_user_accessible());
    assert!(!pte.is_privilege_executable());
    assert_eq!(pte.cache_mode(), 1); // normal cache
    assert!(!pte.is_huge());
    println!("✓ UserMode PTE test passed");
}

#[test]
fn test_pte_kernel_mode() {
    let pte = PteImpl::kernel_mode();
    assert!(pte.valid());
    assert!(pte.is_readable());
    assert!(pte.is_writable());
    assert!(!pte.is_user_executable());
    assert!(!pte.is_user_accessible());
    assert!(pte.is_privilege_executable());
    assert_eq!(pte.cache_mode(), 1); // normal cache
    assert!(!pte.is_huge());
    println!("✓ KernelMode PTE test passed");
}

#[test]
fn test_pte_device_memory() {
    let pte = PteImpl::device_memory();
    assert!(pte.valid());
    assert!(pte.is_readable());
    assert!(pte.is_writable());
    assert!(!pte.is_user_executable());
    assert!(!pte.is_user_accessible());
    assert!(!pte.is_privilege_executable());
    assert_eq!(pte.cache_mode(), 2); // device cache
    assert!(pte.is_huge());
    println!("✓ DeviceMemory PTE test passed");
}

#[test]
fn test_pte_complex_mapping() {
    let mut pg = PageTable::<T4kL3, Fram4k>::new(Fram4k).unwrap();

    // 测试复杂用户映射
    pg.map(&MapConfig {
        vaddr: 0usize.into(),
        paddr: 0x1000usize.into(),
        size: 2 * MB,
        pte: PteImpl::complex_user_mapping(),
        allow_huge: true,
        flush: false,
    }).unwrap();

    // 验证映射成功
    assert!(pg.is_mapped(0usize.into()));
    assert_eq!(pg.translate_phys(0usize.into()).unwrap(), 0x1000usize.into());

    // 测试新的translate方法返回页表项
    let (_, pte) = pg.translate(0usize.into()).unwrap();
    assert!(pte.valid());
    assert!(pte.is_huge());

    println!("✓ Complex mapping test passed");
}