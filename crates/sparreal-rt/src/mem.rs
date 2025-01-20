use core::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut};

use sparreal_kernel::mem::{CMemRange, KernelRegions};

pub unsafe fn clear_bss() {
    unsafe {
        unsafe extern "C" {
            fn _sbss();
            fn _ebss();
        }
        let bss = &mut *slice_from_raw_parts_mut(_sbss as *mut u8, _ebss as usize - _sbss as usize);
        bss.fill(0);
    }
}

pub(crate) fn kernel_regions() -> KernelRegions {
    unsafe extern "C" {
        fn _stext();
        fn _etext();
        fn _srodata();
        fn _erodata();
        fn _sdata();
        fn _edata();
        fn _sbss();
        fn _ebss();
    }

    KernelRegions {
        text: CMemRange {
            start: _stext as usize,
            end: _etext as usize,
        },
        rodata: CMemRange {
            start: _srodata as usize,
            end: _erodata as usize,
        },
        data: CMemRange {
            start: _sdata as usize,
            end: _edata as usize,
        },
        bss: CMemRange {
            start: _sbss as usize,
            end: _ebss as usize,
        },
    }
}

pub fn driver_registers() -> &'static [u8] {
    unsafe extern "C" {
        fn _sdriver();
        fn _edriver();
    }

    unsafe { &*slice_from_raw_parts(_sdriver as *const u8, _edriver as usize - _sdriver as usize) }
}
