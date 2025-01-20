use aarch64_cpu::registers::*;
use core::{
    arch::{asm, global_asm, naked_asm},
    fmt::{self, Debug},
};
use log::*;
use sparreal_kernel::mem::VirtAddr;

use super::context::Context;

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
        "Unhandled serror @ {:#x}: ESR={:#x} (EC {:#08b}, ISS {:#x})",
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
    let reason = if wnr & !cm {
        PageFaultReason::Write
    } else {
        PageFaultReason::Read
    };
    let vaddr = VirtAddr::from(FAR_EL1.get() as usize);

    handle_page_fault(vaddr, reason);
}

#[derive(Debug)]
pub enum PageFaultReason {
    Read,
    Write,
}

pub fn handle_page_fault(vaddr: VirtAddr, reason: PageFaultReason) {
    panic!("Invalid addr fault @{vaddr:?}, reason: {reason:?}");
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

macro_rules! save_task_x {
    () => {
        "
        add x0, x0,   {size}
        stp X29,X30, [x0,#-0x10]!
        stp X27,X28, [x0,#-0x10]!
        stp X25,X26, [x0,#-0x10]!
        stp X23,X24, [x0,#-0x10]!
        stp X21,X22, [x0,#-0x10]!
        stp X19,X20, [x0,#-0x10]!
        sub x0, x0,  #0x90
        mrs	x9, SPSR_EL1
        stp x9, x10,  [x0,#-0x10]!
                "
    };
}

macro_rules! save_task_q {
    () => {
        "
            stp q30, q31,  [x0,#-0x20]!
            stp q28, q29,  [x0,#-0x20]!
            stp q26, q27,  [x0,#-0x20]!
            stp q24, q25,  [x0,#-0x20]!
            stp q22, q23,  [x0,#-0x20]!
            stp q20, q21,  [x0,#-0x20]!
            stp q18, q19,  [x0,#-0x20]!
            stp q16, q17,  [x0,#-0x20]!
            stp q14, q15,  [x0,#-0x20]!
            stp q12, q13,  [x0,#-0x20]!
            stp q10, q11,  [x0,#-0x20]!
            stp q8,  q9,   [x0,#-0x20]!
            stp q6,  q7,   [x0,#-0x20]!
            stp q4,  q5,   [x0,#-0x20]!
            stp q2,  q3,   [x0,#-0x20]!
            stp q0,  q1,   [x0,#-0x20]!
            mrs     x9,  fpcr
            mrs     x10, fpsr
            stp x9,  x10,  [x0,#-0x10]!
                    "
    };
}

// macro_rules! save_task_lr {
//     () => {
//         "
//         mov x9, sp
//         stp x9, lr,    [x0,#-0x10]!
//             "
//     };
// }

macro_rules! save_task_pc_sp {
    () => {
        "
        
    mov x10, lr
    mov x0, sp
    sub x0, x0,   #0x10
	stp x0, x10,  [sp,#-0x10]!
            "
    };
}

macro_rules! restore_task_x {
    () => {
        "
        add x1, x1,   {size}
        ldp X29,X30, [x1,#-0x10]!
        ldp X27,X28, [x1,#-0x10]!
        ldp X25,X26, [x1,#-0x10]!
        ldp X23,X24, [x1,#-0x10]!
        ldp X21,X22, [x1,#-0x10]!
        ldp X19,X20, [x1,#-0x10]!
        sub x1, x1,  #0x90
        ldp x9, x10,  [x1,#-0x10]!
        msr	SPSR_EL1, x9
                "
    };
}

macro_rules! restore_task_q {
    () => {
        "
            ldp q30, q31,  [x1,#-0x20]!
            ldp q28, q29,  [x1,#-0x20]!
            ldp q26, q27,  [x1,#-0x20]!
            ldp q24, q25,  [x1,#-0x20]!
            ldp q22, q23,  [x1,#-0x20]!
            ldp q20, q21,  [x1,#-0x20]!
            ldp q18, q19,  [x1,#-0x20]!
            ldp q16, q17,  [x1,#-0x20]!
            ldp q14, q15,  [x1,#-0x20]!
            ldp q12, q13,  [x1,#-0x20]!
            ldp q10, q11,  [x1,#-0x20]!
            ldp q8,  q9,   [x1,#-0x20]!
            ldp q6,  q7,   [x1,#-0x20]!
            ldp q4,  q5,   [x1,#-0x20]!
            ldp q2,  q3,   [x1,#-0x20]!
            ldp q0,  q1,   [x1,#-0x20]!
            ldp x9,  x10,  [x1,#-0x10]!
            msr      fpcr, x9
            msr     fpsr, x10
            "
    };
}

macro_rules! restore_task_lr {
    () => {
        "
            ldp x9, lr,    [x1,#-0x10]!
            mov sp, x9
            ret"
    };
}
// #[cfg(hard_float)]
// #[naked]
// unsafe extern "C" fn context_switch(_current_task: &mut Context, _next_task: &Context) {
//     unsafe {
//         naked_asm!(
//               //x0
//         save_task_x!(),
//         save_task_q!(),
//         save_task_lr!(),
//               //x1
//         restore_task_x!(),
//         restore_task_q!(),
//         restore_task_lr!(),
//               size = const size_of::<Context>()
//           )
//     }
// }

// #[cfg(not(hard_float))]
// #[naked]
// unsafe extern "C" fn context_switch(_current_task: &mut Context, _next_task: &Context) {
//     unsafe {
//         naked_asm!(
//               //x0
//         save_task_x!(),
//         save_task_lr!(),
//               //x1
//         restore_task_x!(),
//         restore_task_lr!(),
//               size = const size_of::<Context>()
//           )
//     }
// }

#[cfg(hard_float)]
#[naked]
unsafe extern "C" fn context_switch(_current_task: &mut Context, _next_task: &Context) {
    unsafe {
        naked_asm!(
              //x0
            save_x_spsr!(),
            save_q!(),
            save_task_pc_sp!(),
            "",

              //x1
        restore_task_x!(),
        restore_task_q!(),
        restore_task_lr!(),
              size = const size_of::<Context>()
          )
    }
}

// #[cfg(not(hard_float))]
// #[naked]
// unsafe extern "C" fn context_switch(_current_task: &mut Context, _next_task: &Context) {
//     unsafe {
//         naked_asm!(
//               //x0
//         save_task_x!(),
//         save_task_lr!(),
//               //x1
//         restore_task_x!(),
//         restore_task_lr!(),
//               size = const size_of::<Context>()
//           )
//     }
// }
