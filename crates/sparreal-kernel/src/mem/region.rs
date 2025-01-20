use crate::platform_if::PlatformImpl;

pub fn bss() -> &'static [u8] {
    PlatformImpl::kernel_regions().bss.as_slice()
}

pub fn text() -> &'static [u8] {
    PlatformImpl::kernel_regions().text.as_slice()
}
