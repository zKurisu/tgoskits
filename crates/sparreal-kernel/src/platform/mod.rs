use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::CStr;
use core::ops::Range;
use core::ptr::NonNull;
use driver_interface::DriverRegister;

use crate::globals::global_val;
use crate::mem::PhysAddr;
use crate::platform_if::*;
use fdt::Fdt;

pub mod fdt;

pub enum PlatformInfoKind {
    DeviceTree(Fdt),
}

impl PlatformInfoKind {
    pub fn new_fdt(addr: NonNull<u8>) -> Self {
        PlatformInfoKind::DeviceTree(Fdt::new(addr))
    }

    pub fn memorys(&self) -> impl Iterator<Item = Range<PhysAddr>> {
        let mut out: [Option<Range<PhysAddr>>; 24] =
            unsafe { core::mem::MaybeUninit::zeroed().assume_init() };
        let mut len = 0;

        match self {
            PlatformInfoKind::DeviceTree(fdt) => {
                for (i, m) in fdt
                    .get()
                    .memory()
                    .flat_map(|m| m.regions())
                    .map(|r| {
                        let start = PhysAddr::from(r.address as usize);
                        start..start + r.size
                    })
                    .enumerate()
                {
                    if i >= out.len() {
                        break;
                    }
                    out[i] = Some(m);
                    len += 1;
                }
            }
        }

        let mut iter = 0;
        core::iter::from_fn(move || {
            if iter >= len {
                None
            } else {
                let m = out[iter].take().unwrap();
                iter += 1;
                Some(m)
            }
        })
    }

    pub fn main_memory(&self) -> Option<Range<PhysAddr>> {
        let kernel_text = PlatformImpl::kernel_regions().text;

        let mut first = None;

        for m in self.memorys() {
            let r = m.start.as_usize()..m.end.as_usize();
            if r.contains(&kernel_text.end) {
                return Some(m);
            }
            if first.is_none() {
                first = Some(m);
            }
        }

        first
    }

    pub fn debugcon(&self) -> Option<SerialPort> {
        match self {
            Self::DeviceTree(fdt) => fdt.debugcon(),
        }
    }
}

pub fn cpu_list() -> Vec<CPUInfo> {
    match &global_val().platform_info {
        PlatformInfoKind::DeviceTree(fdt) => fdt.cpus(),
    }
}

pub fn cpu_id() -> usize {
    PlatformImpl::cpu_id()
}

pub fn platform_name() -> String {
    match &global_val().platform_info {
        PlatformInfoKind::DeviceTree(fdt) => fdt.model_name().unwrap_or_default(),
    }
}

pub fn shutdown() -> ! {
    PlatformImpl::shutdown();
}

pub fn wait_for_interrupt() {
    PlatformImpl::wait_for_interrupt();
}

pub fn kstack_size() -> usize {
    PlatformImpl::kstack_size()
}

pub fn app_main() {
    unsafe extern "C" {
        fn __sparreal_rt_main();
    }
    unsafe { __sparreal_rt_main() }
}

#[derive(Debug)]
pub struct CPUInfo {
    pub cpu_id: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct SerialPort {
    pub addr: PhysAddr,
    pub size: Option<usize>,
    compatible: [Option<[u8; 128]>; 4],
}

impl SerialPort {
    pub fn new<'a>(
        addr: PhysAddr,
        size: Option<usize>,
        compatibles: impl Iterator<Item = &'a str>,
    ) -> Self {
        let mut compatible_out = [None; 4];

        for (i, c) in compatibles.enumerate() {
            if i == compatible_out.len() {
                break;
            }
            let bytes = c.as_bytes();
            let mut bytes_out = [0u8; 128];
            bytes_out[..bytes.len()].copy_from_slice(bytes);
            compatible_out[i] = Some(bytes_out);
        }

        Self {
            addr,
            size,
            compatible: compatible_out,
        }
    }

    pub fn compatibles(&self) -> impl Iterator<Item = &str> {
        let mut iter = 0;

        core::iter::from_fn(move || {
            if iter >= self.compatible.len() {
                None
            } else {
                let bytes = self.compatible[iter].as_ref()?;
                iter += 1;
                CStr::from_bytes_until_nul(bytes).ok()?.to_str().ok()
            }
        })
    }
}

pub fn module_registers() -> Vec<DriverRegister> {
    PlatformImpl::driver_registers().iter().collect()
}
