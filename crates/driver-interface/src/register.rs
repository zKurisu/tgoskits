use core::ptr::slice_from_raw_parts;

use crate::interrupt_controller;

#[repr(C)]
#[derive(Clone)]
pub struct DriverRegister {
    pub name: &'static str,
    /// split by `\n`
    pub compatibles: &'static str,
    pub probe: ProbeFn,
}

unsafe impl Send for DriverRegister {}
unsafe impl Sync for DriverRegister {}

#[derive(Clone)]
pub enum ProbeFn {
    InterruptController(interrupt_controller::ProbeFn),
    Uart,
}

#[repr(C)]
pub struct DriverRegisterListRef {
    data: *const u8,
    len: usize,
}

impl DriverRegisterListRef {
    pub fn from_raw(data: &'static [u8]) -> Self {
        Self {
            data: data.as_ptr() as _,
            len: data.len(),
        }
    }

    pub fn iter(&self) -> RegisterIter<'static> {
        RegisterIter::new(unsafe { core::slice::from_raw_parts(self.data, self.len) })
    }
}

pub struct RegisterIter<'a> {
    data: &'a [u8],
    iter: usize,
}

impl<'a> RegisterIter<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        RegisterIter { data, iter: 0 }
    }
}

impl<'a> Iterator for RegisterIter<'a> {
    type Item = DriverRegister;

    fn next(&mut self) -> Option<Self::Item> {
        if self.data.is_empty() {
            return None;
        }

        let slice = unsafe {
            &*slice_from_raw_parts(
                self.data.as_ptr() as *const DriverRegister,
                self.data.len() / size_of::<DriverRegister>(),
            )
        };
        let out = slice.get(self.iter).cloned();
        self.iter += 1;
        out
    }
}
