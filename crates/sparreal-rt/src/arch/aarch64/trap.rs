use aarch64_cpu::registers::*;
use log::warn;
use memory_addr::VirtAddr;

#[no_mangle]
unsafe extern "C" fn handle_sync(tf: &TrapFrame) {
    let esr = ESR_EL1.extract();
    let iss = esr.read(ESR_EL1::ISS);
    let elr = ELR_EL1.get();

    if let Some(code) = esr.read_as_enum(ESR_EL1::EC) {
        match code {
            ESR_EL1::EC::Value::SVC64 => {
                warn!("No syscall is supported currently!");
            }
            ESR_EL1::EC::Value::DataAbortLowerEL => handle_data_abort(tf, iss, true),
            ESR_EL1::EC::Value::DataAbortCurrentEL => handle_data_abort(tf, iss, false),
            ESR_EL1::EC::Value::Brk64 => {
                // debug!("BRK #{:#x} @ {:#x} ", iss, tf.elr);
                // tf.elr += 4;
            }
            _ => {
                // panic!(
                //     "Unhandled synchronous exception @ {:#x}: ESR={:#x} (EC {:#08b}, ISS {:#x})",
                //     elr,
                //     esr.get(),
                //     esr.read(ESR_EL1::EC),
                //     esr.read(ESR_EL1::ISS),
                // );
            }
        }
    }
}

#[no_mangle]
unsafe extern "C" fn handle_irq() {
    panic!("irq")
}

#[no_mangle]
unsafe extern "C" fn handle_frq() {
    panic!("frq")
}
fn handle_data_abort(_tf: &TrapFrame, iss: u64, _is_user: bool) {
    let wnr = (iss & (1 << 6)) != 0; // WnR: Write not Read
    let cm = (iss & (1 << 8)) != 0; // CM: Cache maintenance
    let reason = if wnr & !cm {
        PageFaultReason::Write
    } else {
        PageFaultReason::Read
    };
    let vaddr = VirtAddr::from(FAR_EL1.get() as usize);

    handle_page_fault(vaddr, reason);
}

pub fn handle_page_fault(vaddr: VirtAddr, reason: PageFaultReason) {
    panic!("Invalid addr fault @{vaddr:?}, reason: {reason:?}");
}

#[derive(Debug)]
pub enum PageFaultReason {
    Read,
    Write,
}

#[repr(transparent)]
pub struct TrapFrame {
    pub x: [usize; 31],
}
