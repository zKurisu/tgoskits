use core::{
    arch::{asm, global_asm, naked_asm},
    fmt::{self, Debug},
};

use aarch64_cpu::registers::*;
use log::*;

#[unsafe(no_mangle)]
unsafe extern "C" fn __handle_sync(ctx: &Context) {
    let esr = ESR_EL1.extract();
    let iss = esr.read(ESR_EL1::ISS);
    let elr = ctx.pc;

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
                    "\r\n{:?}\r\nUnhandled synchronous exception @ {:#x}: ESR={:#x} (EC {:#08b}, ISS {:#x})",
                    ctx,
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
unsafe extern "C" fn __handle_irq(ctx: &Context) {
    let sp = ctx.sp;
    sparreal_kernel::irq::handle_irq();
    unsafe {
        asm!(
            "mov x0, {0}",
            in(reg) sp,
        );
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn __handle_serror(ctx: &Context) {
    error!("SError exception:");
    let esr = ESR_EL1.extract();
    let _iss = esr.read(ESR_EL1::ISS);
    let elr = ELR_EL1.get();
    error!("{:?}", ctx);
    panic!(
        "Unhandled synchronous exception @ {:#x}: ESR={:#x} (EC {:#08b}, ISS {:#x})",
        elr,
        esr.get(),
        esr.read(ESR_EL1::EC),
        esr.read(ESR_EL1::ISS),
    );
}

#[unsafe(no_mangle)]
unsafe extern "C" fn __handle_fiq() {
    panic!("fiq")
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

pub enum PageFaultReason {
    Read,
    Write,
}

// pub fn handle_page_fault(vaddr: VirtAddr, reason: PageFaultReason) {
//     // panic!("Invalid addr fault @{vaddr:?}, reason: {reason:?}");
// }

#[repr(C, align(0x10))]
#[derive(Clone)]
pub struct Context {
    pub sp: u64,
    pub pc: u64,
    #[cfg(hard_float)]
    /// Floating-point Control Register (FPCR)
    pub fpcr: u64,
    #[cfg(hard_float)]
    /// Floating-point Status Register (FPSR)
    pub fpsr: u64,
    #[cfg(hard_float)]
    pub q: [u128; 32],
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

#[cfg(hard_float)]
macro_rules! save_q {
    () => {
        "
    stp q30, q31,  [sp,#-0x20]!
    stp q28, q29,  [sp,#-0x20]!
    stp q26, q27,  [sp,#-0x20]!
    stp q24, q25,  [sp,#-0x20]!
    stp q22, q23,  [sp,#-0x20]!
    stp q20, q21,  [sp,#-0x20]!
    stp q18, q19,  [sp,#-0x20]!
    stp q16, q17,  [sp,#-0x20]!
    stp q14, q15,  [sp,#-0x20]!
    stp q12, q13,  [sp,#-0x20]!
    stp q10, q11,  [sp,#-0x20]!
    stp q8,  q9,   [sp,#-0x20]!
    stp q6,  q7,   [sp,#-0x20]!
    stp q4,  q5,   [sp,#-0x20]!
    stp q2,  q3,   [sp,#-0x20]!
    stp q0,  q1,   [sp,#-0x20]!
    mrs     x9,  fpcr
    mrs     x10, fpsr
    stp x9,  x10,  [sp,#-0x10]!
"
    };
}

#[cfg(hard_float)]
macro_rules! restore_q {
    () => {
        "
    ldp    x9,  x10, [sp], #0x10
    msr    fpcr, x9
    msr    fpsr, x10
    ldp    q0,  q1,  [sp], #0x20
    ldp    q2,  q3,  [sp], #0x20
    ldp    q4,  q5,  [sp], #0x20
    ldp    q6,  q7,  [sp], #0x20
    ldp    q8,  q9,  [sp], #0x20
    ldp    q10, q11, [sp], #0x20
    ldp    q12, q13, [sp], #0x20
    ldp    q14, q15, [sp], #0x20
    ldp    q16, q17, [sp], #0x20
    ldp    q18, q19, [sp], #0x20
    ldp    q20, q21, [sp], #0x20
    ldp    q22, q23, [sp], #0x20
    ldp    q24, q25, [sp], #0x20
    ldp    q26, q27, [sp], #0x20
    ldp    q28, q29, [sp], #0x20
    ldp    q30, q31, [sp], #0x20
"
    };
}

macro_rules! save_x_spsr {
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
    stp x9, x0, [sp,#-0x10]!
        "
    };
}

macro_rules! save_pc_sp {
    () => {
        "
    mrs x10, ELR_EL1
    mov x0, sp
    sub x0, x0,   #0x10
	stp x0, x10,  [sp,#-0x10]!
        "
    };
}

macro_rules! restore_pc_sp {
    () => {
        "
    mov sp, x0
    ldp X0, X10,    [sp], #0x10
    msr	ELR_EL1,    X10
        "
    };
}

macro_rules! restore_x_spsr {
    () => {
        "
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

#[cfg(hard_float)]
// `handler`返回时，从 `x0` 取出 `sp`，作为栈顶地址
macro_rules! handler {
    ($name:ident, $handler:expr) => {
        #[naked]
        extern "C" fn $name(ctx: &Context) {
        unsafe {
        naked_asm!(
            save_x_spsr!(),
            save_q!(),
            save_pc_sp!(),
            "mov    x0, sp",
            "BL 	{handle}",
            restore_pc_sp!(),
            restore_q!(),
            restore_x_spsr!(),
            "eret",
            handle = sym $handler,
                )
            }
        }
    };
}

#[cfg(not(hard_float))]
// `handler`返回时，从 `x0` 取出 `sp`，作为栈顶地址
macro_rules! handler {
    ($name:ident, $handler:expr) => {
        #[naked]
        extern "C" fn $name(ctx: &Context) {
        unsafe {
        naked_asm!(
            save_x_spsr!(),
            save_pc_sp!(),
            "mov    x0, sp",
            "BL 	{handle}",
            restore_pc_sp!(),
            restore_x_spsr!(),
            "eret",
            handle = sym $handler,
                )
            }
        }
    };
}

handler!(handle_fiq, __handle_fiq);
handler!(handle_irq, __handle_irq);
handler!(handle_sync, __handle_sync);
handler!(handle_serror, __handle_serror);

global_asm!(
    include_str!("vectors.s"),
    irq_handler = sym handle_irq,
    fiq_handler = sym handle_fiq,
    sync_handler = sym handle_sync,
    serror_handler = sym handle_serror,
);
