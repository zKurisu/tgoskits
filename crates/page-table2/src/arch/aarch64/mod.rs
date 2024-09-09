use core::{fmt::Debug, mem::transmute};

use memory_addr::PhysAddr;
use page_table_interface::{GenericPTE, PTEConfig, PageAttribute};

#[allow(unused)]
#[repr(u64)]
pub enum AttrIndex {
    Device = 0,
    Normal = 1,
    NonCacheable = 2,
}

impl AttrIndex {
    #[cfg(target_arch = "aarch64")]
    pub const fn mair_value() -> u64 {
        // Device-nGnRE memory
        use aarch64_cpu::registers::*;
        let attr0 = MAIR_EL1::Attr0_Device::nonGathering_nonReordering_EarlyWriteAck.value;
        // Normal memory
        let attr1 = MAIR_EL1::Attr1_Normal_Inner::WriteBack_NonTransient_ReadWriteAlloc.value
            | MAIR_EL1::Attr1_Normal_Outer::WriteBack_NonTransient_ReadWriteAlloc.value;
        let attr2 = MAIR_EL1::Attr2_Normal_Inner::NonCacheable.value
            | MAIR_EL1::Attr2_Normal_Outer::NonCacheable.value;
        attr0 | attr1 | attr2 // 0x44_ff_04
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct PTE(u64);

impl PTE {
    pub fn set_is_block(&mut self, val: bool) {
        if val {
            self.0 &= !DescriptorAttr::NON_BLOCK.bits();
        } else {
            self.0 |= DescriptorAttr::NON_BLOCK.bits();
        }
    }

    /// Creates an empty descriptor with all bits set to zero.
    pub const fn empty() -> Self {
        Self(0)
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

    fn paddr(&self) -> PhysAddr {
        ((self.0 & DescriptorAttr::PHYS_ADDR_MASK) as usize).into()
    }
}

impl Into<u64> for PTE {
    fn into(self) -> u64 {
        self.0
    }
}

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

impl From<PageAttribute> for DescriptorAttr {
    fn from(value: PageAttribute) -> Self {
        let mut attr = if value.contains(PageAttribute::Device) {
            DescriptorAttr::from_index(AttrIndex::Device)
        } else if value.contains(PageAttribute::NonCache) {
            DescriptorAttr::from_index(AttrIndex::NonCacheable)
        } else {
            DescriptorAttr::from_index(AttrIndex::Normal)
        } | DescriptorAttr::AF;

        if value.contains(PageAttribute::Read) {
            attr |= DescriptorAttr::VALID;
        }
        if !value.contains(PageAttribute::Write) {
            attr |= DescriptorAttr::AP_RO;
        }

        if value.contains(PageAttribute::User) {
            attr |= Self::AP_EL0 | Self::PXN;
            if !value.contains(PageAttribute::Execute) {
                attr |= Self::UXN;
            }
        } else {
            attr |= Self::UXN;
            if !value.contains(PageAttribute::Execute) {
                attr |= Self::PXN;
            }
        }
        attr
    }
}

impl From<DescriptorAttr> for PageAttribute {
    fn from(attr: DescriptorAttr) -> Self {
        if !attr.contains(DescriptorAttr::VALID) {
            return Self::empty();
        }
        let mut flags = Self::Read;
        if !attr.contains(DescriptorAttr::AP_RO) {
            flags |= Self::Write;
        }
        if attr.contains(DescriptorAttr::AP_EL0) {
            flags |= Self::User;
            if !attr.contains(DescriptorAttr::UXN) {
                flags |= Self::Execute;
            }
        } else if !attr.intersects(DescriptorAttr::PXN) {
            flags |= Self::Execute;
        }
        let idx: AttrIndex = unsafe { transmute(attr.attr_index()) };

        match idx {
            AttrIndex::Device => flags |= Self::Device,
            AttrIndex::NonCacheable => flags |= Self::NonCache,
            _ => {}
        }
        flags
    }
}

impl DescriptorAttr {
    const PHYS_ADDR_MASK: u64 = 0x0000_ffff_ffff_f000; // bits 12..48

    pub fn new(mair_idx: u64) -> Self {
        let bits = (mair_idx) << 2;
        Self::from_bits_retain(bits) | Self::VALID
    }

    pub fn attr_index(&self) -> u64 {
        (self.bits() & 0b11100) >> 2
    }

    pub fn from_index(idx: AttrIndex) -> Self {
        Self::new(idx as u64)
    }
}

impl From<PTEConfig> for DescriptorAttr {
    fn from(value: PTEConfig) -> Self {
        let mut des = DescriptorAttr::from(value.attributes);
        if value.is_block {
            des &= !DescriptorAttr::NON_BLOCK;
        } else {
            des |= DescriptorAttr::NON_BLOCK;
        }
        des |=
            DescriptorAttr::from_bits_retain(value.paddr.as_usize() as u64 & Self::PHYS_ADDR_MASK);

        des
    }
}

impl GenericPTE for PTE {
    fn read(&self) -> PTEConfig {
        PTEConfig {
            paddr: PTE::paddr(self),
            is_block: self.is_block(),
            attributes: DescriptorAttr::from_bits_retain(self.0).into(),
        }
    }

    fn set(&mut self, pte: PTEConfig) {
        let des = DescriptorAttr::from(pte);
        self.0 = des.bits();
    }

    fn new_page(pte: PTEConfig) -> Self {
        let des = DescriptorAttr::from(pte);
        Self(des.bits())
    }

    fn paddr(&self) -> PhysAddr {
        PTE::paddr(self)
    }
}

impl Debug for PTE {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let pte = self.read();
        let attr = DescriptorAttr::from_bits_retain(self.0);
        write!(f, "PTE @{:#x}, attrs: {:?}", pte.paddr, attr)
    }
}
