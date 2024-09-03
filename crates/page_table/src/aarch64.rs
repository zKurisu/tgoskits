//! AArch64 VMSAv8-64 translation table format descriptors.

use core::arch::asm;
use memory_addr::PhysAddr;

use crate::GenericPTE;

bitflags::bitflags! {
    /// Memory attribute fields in the VMSAv8-64 translation table format descriptors.
    #[derive(Debug, Clone, Copy)]
    pub struct DescriptorAttr: u64 {
        // Attribute fields in stage 1 VMSAv8-64 Block and Page descriptors:

        /// Whether the descriptor is valid.
        const VALID =       1 << 0;
        /// The descriptor gives the address of the next level of translation table or 4KB page.
        /// (not a 2M, 1G block)
        const NON_BLOCK =   1 << 1;
        /// Memory attributes index field.
        const ATTR_INDX =   0b111 << 2;
        /// Non-secure bit. For memory accesses from Secure state, specifies whether the output
        /// address is in Secure or Non-secure memory.
        const NS =          1 << 5;
        /// Access permission: accessable at EL0.
        const AP_EL0 =      1 << 6;
        /// Access permission: read-only.
        const AP_RO =       1 << 7;
        /// Shareability: Inner Shareable (otherwise Outer Shareable).
        const INNER =       1 << 8;
        /// Shareability: Inner or Outer Shareable (otherwise Non-shareable).
        const SHAREABLE =   1 << 9;
        /// The Access flag.
        const AF =          1 << 10;
        /// The not global bit.
        const NG =          1 << 11;
        /// Indicates that 16 adjacent translation table entries point to contiguous memory regions.
        const CONTIGUOUS =  1 <<  52;
        /// The Privileged execute-never field.
        const PXN =         1 <<  53;
        /// The Execute-never or Unprivileged execute-never field.
        const UXN =         1 <<  54;

        // Next-level attributes in stage 1 VMSAv8-64 Table descriptors:

        /// PXN limit for subsequent levels of lookup.
        const PXN_TABLE =           1 << 59;
        /// XN limit for subsequent levels of lookup.
        const XN_TABLE =            1 << 60;
        /// Access permissions limit for subsequent levels of lookup: access at EL0 not permitted.
        const AP_NO_EL0_TABLE =     1 << 61;
        /// Access permissions limit for subsequent levels of lookup: write access not permitted.
        const AP_NO_WRITE_TABLE =   1 << 62;
        /// For memory accesses from Secure state, specifies the Security state for subsequent
        /// levels of lookup.
        const NS_TABLE =            1 << 63;
    }
}

impl DescriptorAttr {
    pub fn new(mair_idx: u64) -> Self {
        let bits = (mair_idx) << 2;
        Self::from_bits_retain(bits) | Self::VALID
    }
}

pub unsafe fn flush_tlb(vaddr: Option<*mut u8>) {
    if let Some(vaddr) = vaddr {
        asm!("tlbi vaae1is, {}; dsb sy; isb", in(reg) vaddr as usize)
    } else {
        // flush the entire TLB
        asm!("tlbi vmalle1; dsb sy; isb")
    }
}

/// A VMSAv8-64 translation table descriptor.
///
/// Note that the **AttrIndx\[2:0\]** (bit\[4:2\]) field is set to `0` for device
/// memory, and `1` for normal memory. The system must configure the MAIR_ELx
/// system register accordingly.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PTE(u64);

impl PTE {
    const PHYS_ADDR_MASK: u64 = 0x0000_ffff_ffff_f000; // bits 12..48

    /// Creates an empty descriptor with all bits set to zero.
    pub const fn empty() -> Self {
        Self(0)
    }
    pub const fn new(paddr: usize, des: DescriptorAttr) -> Self {
        Self(des.bits() | (paddr as u64 & Self::PHYS_ADDR_MASK))
    }

    pub fn invalid(&self) -> bool {
        self.0 & DescriptorAttr::VALID.bits() == 0
    }
    pub fn set_valid(&mut self) {
        self.0 |= DescriptorAttr::VALID.bits();
    }
    pub fn clear_valid(&mut self) {
        self.0 &= !DescriptorAttr::VALID.bits();
    }

    pub fn is_block(&self) -> bool {
        self.0 & DescriptorAttr::NON_BLOCK.bits() == 0
    }

    pub fn set_is_block(&mut self, val: bool) {
        if val {
            self.0 &= !DescriptorAttr::NON_BLOCK.bits();
        } else {
            self.0 |= DescriptorAttr::NON_BLOCK.bits();
        }
    }

    pub fn paddr(&self) -> usize {
        (self.0 & Self::PHYS_ADDR_MASK) as usize
    }
}

impl GenericPTE for PTE {
    type Attrs = DescriptorAttr;

    fn new_page(paddr: PhysAddr, attrs: Self::Attrs, is_block: bool) -> Self {
        let mut s = PTE::new(
            paddr.as_usize(),
            attrs | DescriptorAttr::VALID | DescriptorAttr::AF,
        );
        s.set_is_block(is_block);
        s
    }

    fn is_block(&self) -> bool {
        PTE::is_block(self)
    }

    fn new_table(paddr: PhysAddr) -> Self {
        PTE::new(
            paddr.as_usize(),
            DescriptorAttr::VALID | DescriptorAttr::NON_BLOCK,
        )
    }

    fn empty() -> Self {
        PTE::empty()
    }

    fn valid(&self) -> bool {
        !PTE::invalid(self)
    }

    fn paddr(&self) -> PhysAddr {
        PTE::paddr(self).into()
    }

    fn clear_valid(&mut self) {
        PTE::clear_valid(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid() {
        let mut pte = PTE::empty();
        assert!(pte.invalid());
        pte.set_valid();
        assert!(!pte.invalid());
    }

    #[test]
    fn test_addr() {
        let pte = PTE::new(
            0x12345678,
            DescriptorAttr::VALID | DescriptorAttr::NON_BLOCK,
        );

        assert_eq!(pte.paddr(), 0x12345678);
    }
}
