#![allow(unused)]

include!(concat!(env!("OUT_DIR"), "/constant.rs"));

pub const MAX_HART_ID: usize = SMP - 1;
pub const STACK_SIZE: usize = HART_STACK_SIZE * SMP;

pub const BYTES_1K: usize = 1024;
pub const BYTES_1M: usize = 1024 * BYTES_1K;
pub const BYTES_1G: usize = 1024 * BYTES_1M;
