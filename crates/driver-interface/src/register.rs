use crate::{interrupt_controller, timer};

#[repr(C)]
#[derive(Clone)]
pub struct DriverRegister {
    pub name: &'static str,
    pub compatibles: &'static [&'static str],
    pub probe: ProbeFnKind,
}

unsafe impl Send for DriverRegister {}
unsafe impl Sync for DriverRegister {}

#[derive(Clone)]
pub enum ProbeFnKind {
    InterruptController(interrupt_controller::ProbeFn),
    Timer(timer::ProbeFn),
}

#[repr(C)]
pub struct DriverRegisterListRef {
    data: *const u8,
    len: usize,
}

impl DriverRegisterListRef {
    pub fn from_raw(data: &'static [u8]) -> Self {
        Self {
            data: data.as_ptr(),
            len: data.len(),
        }
    }

    pub fn as_slice(&self) -> &[DriverRegister] {
        unsafe {
            core::slice::from_raw_parts(self.data as _, self.len / size_of::<DriverRegister>())
        }
    }
}
