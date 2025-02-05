use core::ops::Deref;

use crate::{interrupt_controller, timer};

#[derive(Clone)]
pub struct DriverRegister {
    pub name: &'static str,
    pub probe_kinds: &'static [ProbeKind],
}

unsafe impl Send for DriverRegister {}
unsafe impl Sync for DriverRegister {}

pub enum ProbeKind {
    Fdt {
        compatibles: &'static [&'static str],
        on_probe: OnProbeKindFdt,
    },
}

#[derive(Clone)]
pub enum OnProbeKindFdt {
    InterruptController(interrupt_controller::OnProbeFdt),
    Timer(timer::OnProbeFdt),
}

#[repr(C)]
pub struct DriverRegisterSlice {
    data: *const u8,
    len: usize,
}

impl DriverRegisterSlice {
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

impl Deref for DriverRegisterSlice {
    type Target = [DriverRegister];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}
