use core::{
    arch::{asm, global_asm, naked_asm},
    fmt::{self, Debug},
};

use aarch64_cpu::registers::*;
use log::*;

global_asm!(
    include_str!("vectors.s"),
    irq_handler = sym handle_irq
);

#[unsafe(no_mangle)]
unsafe extern "C" fn __handle_sync(tf: &TrapFrame) {
    let esr = ESR_EL1.extract();
    let iss = esr.read(ESR_EL1::ISS);
    let elr = tf.elr;

    if let Some(code) = esr.read_as_enum(ESR_EL1::EC) {
        match code {
            ESR_EL1::EC::Value::SVC64 => {
                warn!("No syscall is supported currently!");
            }
            ESR_EL1::EC::Value::DataAbortLowerEL => handle_data_abort(iss, true),
            ESR_EL1::EC::Value::DataAbortCurrentEL => handle_data_abort(iss, false),
            ESR_EL1::EC::Value::Brk64 => {
                // debug!("BRK #{:#x} @ {:#x} ", iss, tf.elr);
                // tf.elr += 4;
            }
            _ => {
                panic!(
                    "\r\n{}\r\nUnhandled synchronous exception @ {:#x}: ESR={:#x} (EC {:#08b}, ISS {:#x})",
                    tf,
                    elr,
                    esr.get(),
                    esr.read(ESR_EL1::EC),
                    esr.read(ESR_EL1::ISS),
                );
            }
        }
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn __handle_irq(_ctx: &Context) {
    debug!("{:?}", _ctx);
    debug!("cpsr {:#x}", SPSR_EL1.get());
    debug!("elr {:#x}", ELR_EL1.get());
    debug!("x0: {:p}", _ctx);
    let sp = _ctx.sp;

    sparreal_kernel::irq::handle_irq();

    unsafe {
        asm!(
            "mov x0, {0}",
            in(reg) sp,
        );
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn __handle_serror(tf: &TrapFrame) {
    error!("SError exception:");
    let esr = ESR_EL1.extract();
    let _iss = esr.read(ESR_EL1::ISS);
    let elr = ELR_EL1.get();
    error!("{}", tf);
    panic!(
        "Unhandled synchronous exception @ {:#x}: ESR={:#x} (EC {:#08b}, ISS {:#x})",
        elr,
        esr.get(),
        esr.read(ESR_EL1::EC),
        esr.read(ESR_EL1::ISS),
    );
}

#[unsafe(no_mangle)]
unsafe extern "C" fn __handle_frq() {
    panic!("frq")
}
fn handle_data_abort(iss: u64, _is_user: bool) {
    let wnr = (iss & (1 << 6)) != 0; // WnR: Write not Read
    let cm = (iss & (1 << 8)) != 0; // CM: Cache maintenance
    let _reason = if wnr & !cm {
        PageFaultReason::Write
    } else {
        PageFaultReason::Read
    };
    // let vaddr = VirtAddr::from(FAR_EL1.get() as usize);

    // handle_page_fault(vaddr, reason);
}

// pub fn handle_page_fault(vaddr: VirtAddr, reason: PageFaultReason) {
//     // panic!("Invalid addr fault @{vaddr:?}, reason: {reason:?}");
// }

#[derive(Debug)]
pub enum PageFaultReason {
    Read,
    Write,
}

#[derive(Debug)]
#[repr(C)]
pub struct TrapFrame {
    pub spsr: u64,
    pub sp: u64,
    pub cpacr: u64,
    pub elr: u64,
    pub q: [u64; 64],
    pub x29: u64,
    pub x30: u64,
    pub x18: u64,
    pub x19: u64,
    pub x16: u64,
    pub x17: u64,
    pub x14: u64,
    pub x15: u64,
    pub x12: u64,
    pub x13: u64,
    pub x10: u64,
    pub x11: u64,
    pub x8: u64,
    pub x9: u64,
    pub x6: u64,
    pub x7: u64,
    pub x4: u64,
    pub x5: u64,
    pub x2: u64,
    pub x3: u64,
    pub x0: u64,
    pub x1: u64,
}

impl fmt::Display for TrapFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "TrapFrame:")?;
        writeln!(f, "  spsr: {:#x}", self.spsr)?;
        writeln!(f, "  sp: {:#x}", self.sp)?;
        writeln!(f, "  cpacr: {:#x}", self.cpacr)?;
        writeln!(f, "  elr: {:#x}", self.elr)?;

        writeln!(
            f,
            "  x00: {:#x} x01: {:#x} x02: {:#x} x03: {:#x}",
            self.x0, self.x1, self.x2, self.x3
        )?;
        writeln!(
            f,
            "  x04: {:#x} x05: {:#x} x06: {:#x} x07: {:#x}",
            self.x4, self.x5, self.x6, self.x7
        )?;
        writeln!(
            f,
            "  x08: {:#x} x09: {:#x} x10: {:#x} x11: {:#x}",
            self.x8, self.x9, self.x10, self.x11
        )?;
        writeln!(
            f,
            "  x12: {:#x} x13: {:#x} x14: {:#x} x15: {:#x}",
            self.x12, self.x13, self.x14, self.x15
        )?;
        writeln!(
            f,
            "  x16: {:#x} x17: {:#x} x18: {:#x} x19: {:#x}",
            self.x16, self.x17, self.x18, self.x19
        )?;
        writeln!(f, "  x29: {:#x} x30: {:#x} ", self.x29, self.x30)
    }
}

#[repr(C, align(0x10))]
#[derive(Clone)]
pub struct Context {
    pub sp: u64,
    pub pc: u64,
    pub spsr: u64,
    pub x: [u64; 31],
}

impl Debug for Context {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Context:")?;

        const NUM_CHUNKS: usize = 4;

        for (r, chunk) in self.x.chunks(NUM_CHUNKS).enumerate() {
            let row_start = r * NUM_CHUNKS;

            for (i, v) in chunk.iter().enumerate() {
                let i = row_start + i;
                write!(f, "  x{:<3}: {:#18x}", i, v)?;
            }
            writeln!(f)?;
        }
        writeln!(f, "  spsr: {:#18x}", self.spsr)?;
        writeln!(f, "  pc  : {:#18x}", self.pc)?;
        writeln!(f, "  sp  : {:#18x}", self.sp)
    }
}

macro_rules! save_ctx_base {
    () => {
        "
	stp X29,X30, [sp,#-0x10]!
	stp X27,X28, [sp,#-0x10]!
    stp X25,X26, [sp,#-0x10]!
	stp X23,X24, [sp,#-0x10]!
    stp X21,X22, [sp,#-0x10]!
	stp X19,X20, [sp,#-0x10]!
	stp	X17,X18, [sp,#-0x10]!
	stp	X15,X16, [sp,#-0x10]!
	stp	X13,X14, [sp,#-0x10]!
	stp	X11,X12, [sp,#-0x10]!
	stp	X9,X10,  [sp,#-0x10]!
	stp	X7,X8,   [sp,#-0x10]!
	stp	X5,X6,   [sp,#-0x10]!
	stp	X3,X4,   [sp,#-0x10]!
    stp	X1,X2,   [sp,#-0x10]!
    mrs	x9, SPSR_EL1
    mrs x10, ELR_EL1
	stp x9, x0, [sp,#-0x10]!
    mov x0, sp
    sub x0, x0,   #0x10
	stp x0, x10,  [sp,#-0x10]!
"
    };
}

macro_rules! restore_ctx_base {
    () => {
        "
    mov sp, x0
    ldp X0, X10,    [sp], #0x10
    msr	ELR_EL1,    X10
    ldp X9,X0,      [sp], #0x10
    msr	SPSR_EL1,   X9
	ldp	X1,X2,      [sp], #0x10
    ldp X3,X4,      [sp], #0x10
	ldp X5,X6,      [sp], #0x10
	ldp	X7,X8,      [sp], #0x10
	ldp	X9,X10,     [sp], #0x10
	ldp	X11,X12,    [sp], #0x10
	ldp	X13,X14,    [sp], #0x10
	ldp	X15,X16,    [sp], #0x10
	ldp	X17,X18,    [sp], #0x10
	ldp	X19,x20,    [sp], #0x10
	ldp	X21,X22,    [sp], #0x10
	ldp	X23,X24,    [sp], #0x10
	ldp	X25,X26,    [sp], #0x10
	ldp	X27,X28,    [sp], #0x10
	ldp	X29,X30,    [sp], #0x10
        "
    };
}

#[naked]
extern "C" fn handle_irq(ctx: Context) {
    unsafe {
        naked_asm!(
            save_ctx_base!(),
            "mov    x0, sp",
            "BL 	{handle}",
            restore_ctx_base!(),
            "eret",
            handle = sym __handle_irq,
        )
    }
}
