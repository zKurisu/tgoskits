#![allow(unused)]

include!(concat!(env!("OUT_DIR"), "/constant.rs"));

pub const MAX_HART_ID: usize = SMP - 1;
pub const STACK_SIZE: usize = HART_STACK_SIZE * SMP;
