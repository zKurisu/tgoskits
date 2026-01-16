use aarch64_cpu::registers::{Readable as _, *};
use aarch64_cpu_ext::registers::ICC_SRE_EL2;
use arm_gic_driver::{
    IntId,
    v3::{ICC_IAR1_EL1, ICC_SRE_EL1, Readable as _, ack1, dir, eoi_mode, eoi1},
};
use core::arch::{asm, global_asm};
use kasm_aarch64::aarch64_trap_handler;
use log::*;

use super::context::Context;

#[aarch64_trap_handler(kind = "irq")]
fn handle_irq(_ctx: &Context) {
    if !ICC_SRE_EL1.is_set(ICC_SRE_EL1::SRE) {
        panic!("GIC CPU interface not enabled in EL1!");
    }
    handle_irq_v3_el1();
}

fn handle_irq_v3_el1() {
    let ack = ack1();

    crate::irq::handle_irq(crate::irq::IrqId::new(ack.to_u32() as _));

    if !ack.is_special() {
        eoi1(ack);
        if eoi_mode() {
            dir(ack);
        }
    }
}

#[aarch64_trap_handler(kind = "fiq")]
fn handle_fiq(_ctx: &Context) {}

#[aarch64_trap_handler(kind = "sync")]
fn handle_sync(ctx: &Context) {
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
                    "\r\n{:?}\r\nUnhandled synchronous exception @ {:p}: ESR={:#x} (EC {:#08b}, ISS {:#x})",
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

#[aarch64_trap_handler(kind = "serror")]
fn handle_serror(ctx: &Context) {
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

fn handle_data_abort(iss: u64, _is_user: bool) {
    let wnr = (iss & (1 << 6)) != 0; // WnR: Write not Read
    let cm = (iss & (1 << 8)) != 0; // CM: Cache maintenance
    let reason = if wnr & !cm {
        PageFaultReason::Write
    } else {
        PageFaultReason::Read
    };
    let vaddr = FAR_EL1.get() as usize;
    let pc = ELR_EL1.get();

    panic!("Invalid addr fault @{vaddr:#x}, reason: {reason:?}, pc={pc:#x}");
}

#[derive(Debug)]
pub enum PageFaultReason {
    Read,
    Write,
}

global_asm!(
    include_str!("vectors.s"),
    irq_handler = sym handle_irq,
    fiq_handler = sym handle_fiq,
    sync_handler = sym handle_sync,
    serror_handler = sym handle_serror,
);

pub fn setup() {
    let addr = ext_sym_addr!(__vector_table);

    match CurrentEL.read(CurrentEL::EL) {
        1 => unsafe {
            asm!("msr vbar_el1, {0}", in(reg) addr);
        },
        2 => unsafe {
            asm!("msr vbar_el2, {0}", in(reg) addr);
        },
        _ => panic!("Unsupported exception level for vector table setup"),
    }
}
