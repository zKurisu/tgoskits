use core::cell::UnsafeCell;

use aux_mini::AuxMini;
use pl011::Pl011;
use sparreal_kernel::platform::SerialPort;

mod aux_mini;
mod pl011;

static mut REG_BASE: usize = 0;
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
}

fn uart() -> &'static Uart {
    unsafe { &*UART.0.get() }
}

pub unsafe fn mmu_add_offset(va_offset: usize) {
    unsafe { REG_BASE += va_offset };
}

pub fn put(byte: u8) {
    unsafe {
        match uart() {
            Uart::Pl011(uart) => uart.write(REG_BASE, byte),
            Uart::AuxMini(uart) => uart.write(REG_BASE, byte),
            Uart::None => {}
        }
    }
}
pub fn init_by_info(s: SerialPort) -> Option<()> {
    unsafe { REG_BASE = s.addr.as_usize() };

    for c in s.compatibles() {
        if c.contains("brcm,bcm2835-aux-uart") {
            UART.set(Uart::AuxMini(aux_mini::AuxMini {}));
            break;
        }

        if c.contains("arm,pl011") {
            UART.set(Uart::Pl011(Pl011 {}));
            break;
        }

        if c.contains("arm,primecell") {
            UART.set(Uart::Pl011(Pl011 {}));
            break;
        }
    }

    Some(())
}

enum Uart {
    None,
    Pl011(Pl011),
    AuxMini(AuxMini),
}
