static mut VA_OFFSET: usize = 0;

pub unsafe fn set_va_offset(v: usize) {
    VA_OFFSET = v;
}

pub fn va_offset() -> usize {
    unsafe { VA_OFFSET }
}
