use loongArch64::register::{ecfg, eentry, tlbrentry};

use crate::{arch::cache::local_flush_icache_range, mem::StaticCell};

const VECSIZE: usize = 0x200;

// 等效于 C: long exception_handlers[VECSIZE * 128 / sizeof(long)] __aligned(SZ_64K);
// 在 64 位系统中，sizeof(long) = 8，所以数组大小为 VECSIZE * 128 / 8 = VECSIZE * 16
#[repr(C, align(65536))] // 65536 = 64KB 对齐
struct ExceptionHandlers {
    data: [usize; VECSIZE * 16],
}

impl ExceptionHandlers {
    const fn new() -> Self {
        Self {
            data: [0; VECSIZE * 16],
        }
    }
}

static EXCEPTION_HANDLERS: StaticCell<ExceptionHandlers> =
    StaticCell::new(Some(ExceptionHandlers::new()));

fn eentry_addr() -> usize {
    EXCEPTION_HANDLERS.data.as_ptr() as usize
}

fn tlbrentry_addr() -> usize {
    eentry_addr() + 80 * VECSIZE
}

pub fn per_cpu_trap_init(is_primary: bool) {
    setup_vint_size();
    configure_exception_vector();

    if is_primary {
        for i in 0..64 {
            set_handler(i * VECSIZE, handle_reserved);
        }
        for i in 64..=64 + 14 {
            set_handler(i * VECSIZE, handle_int);
        }

        local_flush_icache_range(eentry_addr(), eentry_addr() + 0x400);
    }
}

fn setup_vint_size() {
    let n = 64 - (VECSIZE / 4).leading_zeros() - 1;
    ecfg::set_vs(n as _);
}

/// 配置异常向量
/// 等效于 C 的 configure_exception_vector()
fn configure_exception_vector() {
    eentry::set_eentry(eentry_addr());
    tlbrentry::set_tlbrentry(tlbrentry_addr());
}

fn set_handler(offset: usize, handler: fn()) {
    unsafe {
        let src = core::slice::from_raw_parts(handler as *const u8, VECSIZE);
        let dst = core::slice::from_raw_parts_mut((eentry_addr() as *mut u8).add(offset), VECSIZE);
        dst.copy_from_slice(src);
        local_flush_icache_range(
            dst.as_ptr_range().start as usize,
            dst.as_ptr_range().end as usize,
        );
    }
}

fn handle_reserved() {}

fn handle_int() {}
