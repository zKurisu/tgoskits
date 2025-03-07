use core::{
    cell::UnsafeCell,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering},
};

use aux_mini::AuxMini;
use fdt_parser::Fdt;
use pl011::Pl011;
use sparreal_kernel::{io::print::*, mem::*};

mod aux_mini;
mod pl011;

static REG_BASE: AtomicUsize = AtomicUsize::new(0);
static UART: UartWapper = UartWapper(UnsafeCell::new(Uart::None));

struct UartWapper(UnsafeCell<Uart>);

unsafe impl Send for UartWapper {}
unsafe impl Sync for UartWapper {}

impl UartWapper {
    fn set(&self, uart: Uart) {
        unsafe {
            *self.0.get() = uart;
        }
    }

    fn get(&self) -> &Uart {
        unsafe { &*self.0.get() }
    }
}

pub unsafe fn mmu_add_offset(va_offset: usize) {
    let new_reg = reg() + va_offset;
    early_dbg("uart set to ");
    early_dbg_hexln(new_reg as _);
    REG_BASE.store(new_reg, Ordering::Release);
}

pub fn reg() -> usize {
    REG_BASE.load(Ordering::Relaxed)
}

pub fn put(byte: u8) {
    match UART.get() {
        Uart::Pl011(uart) => uart.write(reg(), byte),
        Uart::AuxMini(uart) => uart.write(reg(), byte),
        Uart::None => {}
    }
}
pub fn init_by_fdt(fdt: *mut u8) -> Option<()> {
    let fdt = Fdt::from_ptr(NonNull::new(fdt)?).ok()?;
    if let Some((uart, addr)) = fdt_stdout(fdt.clone()) {
        UART.set(uart);
        REG_BASE.store(addr.into(), Ordering::SeqCst);
    }

    Some(())
}

fn fdt_stdout(fdt: Fdt<'_>) -> Option<(Uart, PhysAddr)> {
    let stdout = fdt.chosen()?.stdout()?;
    let reg = stdout.node.reg()?.next()?;
    let addr = PhysAddr::new(reg.address as usize);
    for c in stdout.node.compatibles() {
        if c.contains("brcm,bcm2835-aux-uart") {
            return Some((Uart::AuxMini(aux_mini::AuxMini {}), addr));
        }

        if c.contains("arm,pl011") || c.contains("arm,primecell") {
            return Some((Uart::Pl011(Pl011 {}), addr));
        }
    }

    None
}

enum Uart {
    None,
    Pl011(Pl011),
    AuxMini(AuxMini),
}
