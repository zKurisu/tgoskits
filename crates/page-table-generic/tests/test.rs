//! Mock implementations for testing
//!
//! This module provides mock implementations used in tests for the page-table-generic crate.
#![cfg(not(target_os = "none"))]

use std::alloc::{self, Layout};
use page_table_generic::*;

const MB: usize = 1024 * 1024;

#[test]
fn test_base() {}

/// Mock Page Table Entry for testing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MockPte {
    bits: usize,
}

impl MockPte {
    pub fn new() -> Self {
        Self { bits: 0 }
    }

    pub fn with_flags(valid: bool, huge: bool) -> Self {
        let mut pte = Self { bits: 0 };
        if valid {
            pte.bits |= 1 << 63; // Valid bit at position 63
        }
        if huge {
            pte.bits |= 1 << 10; // Huge bit at position 10
        }
        pte
    }
}

impl Default for MockPte {
    fn default() -> Self {
        Self::new()
    }
}

impl PageTableEntry for MockPte {
    fn valid(&self) -> bool {
        self.bits & (1 << 63) != 0
    }

    fn paddr(&self) -> PhysAddr {
        PhysAddr::new(self.bits & ((1 << 48) - 1)) // 48-bit physical address
    }

    fn set_paddr(&mut self, paddr: PhysAddr) {
        self.bits = (self.bits & !((1 << 48) - 1)) | (paddr.raw() & ((1 << 48) - 1));
    }

    fn set_valid(&mut self, valid: bool) {
        if valid {
            self.bits |= 1 << 63;
        } else {
            self.bits &= !(1 << 63);
        }
    }

    fn is_huge(&self) -> bool {
        self.bits & (1 << 10) != 0
    }

    fn set_is_huge(&mut self, huge: bool) {
        if huge {
            self.bits |= 1 << 10;
        } else {
            self.bits &= !(1 << 10);
        }
    }
}

/// Mock Table Generic configuration for testing
#[derive(Debug, Clone, Copy)]
pub struct MockTableGeneric;

impl TableGeneric for MockTableGeneric {
    type P = MockPte;

    const PAGE_SIZE: usize = 4096; // 4KB
    const LEVEL: usize = 4; // 4级页表
    const MAX_BLOCK_LEVEL: usize = 2; // 支持大页到第2级

    fn flush(_vaddr: Option<VirtAddr>) {
        // 模拟TLB刷新
    }
}

/// Mock allocator for testing
#[derive(Debug, Clone, Copy)]
pub struct MockAllocator4K;

impl FramAllocator for MockAllocator4K {
    fn alloc_frame(&self) -> Option<PhysAddr> {
        let ptr = unsafe { alloc::alloc(Layout::from_size_align(4096, 4096).unwrap()) };
        if ptr.is_null() {
            None
        } else {
            Some(PhysAddr::new(ptr as usize))
        }
    }

    fn dealloc_frame(&self, frame: PhysAddr) {
        unsafe {
            alloc::dealloc(
                frame.raw() as *mut u8,
                Layout::from_size_align(4096, 4096).unwrap(),
            )
        };
    }

    fn phys_to_virt(&self, paddr: PhysAddr) -> *mut u8 {
        paddr.raw() as *mut u8
    }
}

#[test]
fn test_pte_basic_operations() {
    let mut pte = MockPte::new();

    // Test initial state
    assert!(!pte.valid());
    assert!(!pte.is_huge());
    assert_eq!(pte.paddr(), PhysAddr::new(0));

    // Test setting valid
    pte.set_valid(true);
    assert!(pte.valid());

    // Test setting huge
    pte.set_is_huge(true);
    assert!(pte.is_huge());

    // Test setting physical address
    let test_paddr = PhysAddr::new(0x12345000);
    pte.set_paddr(test_paddr);
    assert_eq!(pte.paddr(), test_paddr);

    // Test with_flags
    let pte2 = MockPte::with_flags(true, true);
    assert!(pte2.valid());
    assert!(pte2.is_huge());
}

#[test]
fn test_mock_allocator() {
    let allocator = MockAllocator4K;

    // Test allocation
    let paddr = allocator.alloc_frame().expect("Failed to allocate frame");
    assert_ne!(paddr.raw(), 0);
    assert_eq!(paddr.raw() % 4096, 0); // Should be page aligned

    // Test phys_to_virt
    let vaddr = allocator.phys_to_virt(paddr);
    assert!(!vaddr.is_null());

    // Test deallocation
    allocator.dealloc_frame(paddr);
}

#[test]
fn test_page_table_creation() {
    let allocator = MockAllocator4K;
    let page_table = PageTable::<MockTableGeneric, MockAllocator4K>::new(allocator);
    assert!(page_table.is_ok());
}

#[test]
fn test_simple_mapping() {
    let allocator = MockAllocator4K;
    let mut page_table = PageTable::<MockTableGeneric, MockAllocator4K>::new(allocator).unwrap();

    let config = MapConfig {
        vaddr: VirtAddr::new(0x1000_0000),
        paddr: PhysAddr::new(0x2000_0000),
        size: 0x1000, // 4KB
        pte: MockPte::new(),
        allow_huge: false,
        flush: false,
    };

    let result = page_table.map(&config);
    assert!(result.is_ok());
}

#[test]
fn test_huge_page_mapping() {
    let allocator = MockAllocator4K;
    let mut page_table = PageTable::<MockTableGeneric, MockAllocator4K>::new(allocator).unwrap();

    let config = MapConfig {
        vaddr: VirtAddr::new(0x0), // Must be aligned to 2MB for level 2 huge page
        paddr: PhysAddr::new(0x0),
        size: 2 * MB, // 2MB huge page
        pte: MockPte::new(),
        allow_huge: true,
        flush: false,
    };

    let result = page_table.map(&config);
    assert!(result.is_ok());
}

#[test]
fn test_multiple_page_mapping() {
    let allocator = MockAllocator4K;
    let mut page_table = PageTable::<MockTableGeneric, MockAllocator4K>::new(allocator).unwrap();

    let config = MapConfig {
        vaddr: VirtAddr::new(0x1000_0000),
        paddr: PhysAddr::new(0x2000_0000),
        size: 0x2000, // 8KB (2 pages)
        pte: MockPte::new(),
        allow_huge: false,
        flush: false,
    };

    let result = page_table.map(&config);
    assert!(result.is_ok());
}

#[test]
fn test_alignment_validation() {
    let allocator = MockAllocator4K;
    let mut page_table = PageTable::<MockTableGeneric, MockAllocator4K>::new(allocator).unwrap();

    // Test misaligned virtual address
    let config = MapConfig {
        vaddr: VirtAddr::new(0x1000_1001), // Not page aligned
        paddr: PhysAddr::new(0x2000_0000),
        size: 0x1000,
        pte: MockPte::new(),
        allow_huge: false,
        flush: false,
    };

    let result = page_table.map(&config);
    assert!(result.is_err());
}

#[test]
fn test_zero_size_mapping() {
    let allocator = MockAllocator4K;
    let mut page_table = PageTable::<MockTableGeneric, MockAllocator4K>::new(allocator).unwrap();

    let config = MapConfig {
        vaddr: VirtAddr::new(0x1000_0000),
        paddr: PhysAddr::new(0x2000_0000),
        size: 0, // Zero size
        pte: MockPte::new(),
        allow_huge: false,
        flush: false,
    };

    let result = page_table.map(&config);
    assert!(result.is_err());
}

#[test]
fn test_table_generic_constants() {
    // Test MockTableGeneric constants
    assert_eq!(MockTableGeneric::PAGE_SIZE, 4096);
    assert_eq!(MockTableGeneric::LEVEL, 4);
    assert_eq!(MockTableGeneric::MAX_BLOCK_LEVEL, 2);
}

#[test]
fn test_address_operations() {
    // Test VirtAddr operations
    let vaddr = VirtAddr::new(0x1000_0000);
    let vaddr2 = vaddr + 0x1000;
    assert_eq!(vaddr2.raw(), 0x1000_1000);

    let vaddr3 = vaddr2 - 0x1000;
    assert_eq!(vaddr3.raw(), 0x1000_0000);

    // Test PhysAddr operations
    let paddr = PhysAddr::new(0x2000_0000);
    let paddr2 = paddr + 0x1000;
    assert_eq!(paddr2.raw(), 0x2000_1000);

    let paddr3 = paddr2 - 0x1000;
    assert_eq!(paddr3.raw(), 0x2000_0000);
}

#[test]
fn test_error_types() {
    // Test PagingError creation and display
    let no_mem = PagingError::NoMemory;
    assert_eq!(no_mem.to_string(), "Memory allocation failed");

    let align_err = PagingError::alignment_error("test alignment");
    let align_str = align_err.to_string();
    assert!(align_str.contains("AlignmentError") || align_str.contains("alignment error"));

    let conflict_err = PagingError::mapping_conflict(
        VirtAddr::new(0x1000_0000),
        PhysAddr::new(0x2000_0000),
    );
    let conflict_str = conflict_err.to_string();
    assert!(conflict_str.contains("Mapping conflict"));
    assert!(conflict_str.contains("0x10000000"));
    assert!(conflict_str.contains("0x20000000"));

    let overflow_err = PagingError::address_overflow("test overflow");
    let overflow_str = overflow_err.to_string();
    assert!(overflow_str.contains("Address overflow"));

    let size_err = PagingError::invalid_size("test size");
    let size_str = size_err.to_string();
    assert!(size_str.contains("Invalid mapping size"));

    let hierarchy_err = PagingError::hierarchy_error("test hierarchy");
    let hierarchy_str = hierarchy_err.to_string();
    assert!(hierarchy_str.contains("Page table hierarchy error"));
}