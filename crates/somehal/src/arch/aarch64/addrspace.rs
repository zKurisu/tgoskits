#[cfg(not(feature = "hv"))]
pub const LINER_OFFSET: usize = 0xffff_0000_0000_0000;
#[cfg(feature = "hv")]
pub const LINER_OFFSET: usize = 0;
