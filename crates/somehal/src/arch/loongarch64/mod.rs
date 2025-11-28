#[macro_use]
mod _macros;

mod addrspace;
mod cache;
mod context;
pub(crate) mod entry;
mod head;
mod register;
mod relocate;
mod trap;

use loongArch64::{
    register::{crmd, tcfg, ticlr},
    time,
};
pub use relocate::relocate;

use crate::{ArchTrait, arch::register::irq::TI};

const MIN_TICKS: u64 = 4;
const NS_PER_SEC: u128 = 1_000_000_000;

pub struct Arch;

impl ArchTrait for Arch {
    fn kernel_code() -> &'static [u8] {
        let start = ext_sym_addr!(_head);
        let end = ext_sym_addr!(__kernel_code_end);
        unsafe { core::slice::from_raw_parts(start as *const u8, end - start) }
    }

    fn post_allocator() {}

    fn _pa(vaddr: *const u8) -> usize {
        addrspace::to_phys(vaddr as usize)
    }

    fn _va(paddr: usize) -> *mut u8 {
        addrspace::to_cache(paddr) as *mut u8
    }

    fn ioremap(paddr: usize, _size: usize) -> *mut u8 {
        Self::_io(paddr)
    }

    fn _io(paddr: usize) -> *mut u8 {
        addrspace::to_uncache(paddr) as *mut u8
    }

    fn per_cpu_trap_init(is_primary: bool) {
        trap::per_cpu_trap_init(is_primary);
    }

    fn systimer_irq() -> usize {
        TI as _
    }

    fn systimer_enable() {
        tcfg::set_en(true);
    }

    fn systimer_disable() {
        tcfg::set_en(false);
    }

    fn systimer_set_next_event(interval_ns: u64) {
        let ticks = interval_ns_to_ticks(interval_ns);
        tcfg::set_en(false);
        tcfg::set_periodic(false);
        tcfg::set_init_val(ticks);
        ticlr::clear_timer_interrupt();
        tcfg::set_en(true);
    }

    fn shutdown() -> ! {
        loop {
            unsafe { loongArch64::asm::idle() };
        }
    }

    fn irq_all_is_enabled() -> bool {
        crmd::read().ie()
    }

    fn irq_all_set_enable(enable: bool) {
        crmd::set_ie(enable);
    }
}

fn interval_ns_to_ticks(interval_ns: u64) -> usize {
    if interval_ns == 0 {
        return MIN_TICKS as usize;
    }

    let freq = time::get_timer_freq() as u128;
    if freq == 0 {
        return MIN_TICKS as usize;
    }

    let mut ticks = (freq * interval_ns as u128) / NS_PER_SEC;
    if ticks < MIN_TICKS as u128 {
        ticks = MIN_TICKS as u128;
    }

    // Ensure the value is aligned to a multiple of 4 as required by TCFG
    ticks = (ticks + 3) & !3u128;

    ticks = ticks.min(usize::MAX as u128);
    ticks as usize
}
