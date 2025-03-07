use arrayvec::ArrayVec;
use page_table_generic::{AccessSetting, CacheSetting};

use crate::platform_if::{MMUImpl, PlatformImpl};

use super::{mmu::BootRegion, once::OnceStatic};

const MAX_BOOT_RSV_SIZE: usize = 12;
pub type BootRsvRegionVec = ArrayVec<BootRegion, MAX_BOOT_RSV_SIZE>;

static BOOT_RSV_REGION: OnceStatic<BootRsvRegionVec> = OnceStatic::new(ArrayVec::new_const());

pub(crate) unsafe fn init_boot_rsv_region() {
    unsafe {
        let rsv_regions = MMUImpl::boot_regions();
        BOOT_RSV_REGION.set(rsv_regions);
    }
}

pub fn boot_regions() -> &'static BootRsvRegionVec {
    &BOOT_RSV_REGION
}
