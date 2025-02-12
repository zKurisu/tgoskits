#![allow(unused)]

use core::{
    cell::UnsafeCell,
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::collections::btree_map::BTreeMap;
use driver_interface::intc::CpuId;
use percpu::PerCPU;

pub use crate::platform::PlatformInfoKind;
use crate::{
    mem::PhysAddr,
    platform::{self, cpu_list},
};

mod percpu;

pub struct GlobalVal {
    pub platform_info: PlatformInfoKind,
    pub kstack_top: PhysAddr,
    pub main_memory: Range<PhysAddr>,
    percpu: BTreeMap<CpuId, percpu::PerCPU>,
}

struct LazyGlobal {
    g_ok: AtomicBool,
    cpu_ok: AtomicBool,
    g: UnsafeCell<Option<GlobalVal>>,
}

unsafe impl Sync for LazyGlobal {}

static GLOBAL: LazyGlobal = LazyGlobal::new();

pub fn global_val() -> &'static GlobalVal {
    global_val_meybeuninit().expect("GlobalVal is not init!")
}

pub fn global_val_meybeuninit() -> Option<&'static GlobalVal> {
    if !GLOBAL.g_ok.load(Ordering::SeqCst) {
        return None;
    }
    Some(unsafe { (*GLOBAL.g.get()).as_ref().unwrap() })
}

impl LazyGlobal {
    const fn new() -> Self {
        Self {
            g_ok: AtomicBool::new(false),
            cpu_ok: AtomicBool::new(false),
            g: UnsafeCell::new(None),
        }
    }
}

/// 修改全局变量
///
/// # Safty
/// 只能在其他CPU启动前调用
pub(crate) unsafe fn edit(f: impl FnOnce(&mut GlobalVal)) {
    unsafe {
        let global = (*GLOBAL.g.get()).as_mut().unwrap();
        f(global);
    }
}

unsafe fn get_mut() -> &'static mut GlobalVal {
    unsafe { (*GLOBAL.g.get()).as_mut().unwrap() }
}

/// # Safty
/// 只能在其他CPU启动前调用
pub(crate) unsafe fn setup(platform_info: PlatformInfoKind) -> Result<(), &'static str> {
    let main_memory = platform_info
        .main_memory()
        .ok_or("No memory in platform info")?;

    let g = GlobalVal {
        platform_info,
        kstack_top: main_memory.end,
        main_memory,
        percpu: Default::default(),
    };

    unsafe {
        GLOBAL.g.get().write(Some(g));
        GLOBAL.g_ok.store(true, Ordering::SeqCst);

        match &mut get_mut().platform_info {
            PlatformInfoKind::DeviceTree(fdt) => {
                fdt.setup()?;
            }
        }
    }
    Ok(())
}

/// #Safty
/// 需要在内存初始化完成之后调用
pub(crate) unsafe fn setup_percpu() {
    let cpus = cpu_list();
    let g = unsafe { get_mut() };
    for cpu in cpus {
        let percpu = PerCPU::default();
        g.percpu.insert(cpu.cpu_id, percpu);
    }
    GLOBAL.cpu_ok.store(true, Ordering::SeqCst);
}

pub(crate) fn cpu_global() -> &'static PerCPU {
    cpu_global_meybeuninit().expect("CPU global is not init!")
}

pub(crate) fn cpu_global_meybeuninit() -> Option<&'static PerCPU> {
    if !GLOBAL.cpu_ok.load(Ordering::SeqCst) {
        return None;
    }

    let g = unsafe { get_mut() };
    Some(g.percpu.get(&platform::cpu_id()).unwrap())
}

pub(crate) unsafe fn cpu_global_mut() -> &'static mut PerCPU {
    unsafe { cpu_global_mut_meybeunint().expect("CPU global is not init!") }
}

pub(crate) unsafe fn cpu_global_mut_meybeunint() -> Option<&'static mut PerCPU> {
    if !GLOBAL.cpu_ok.load(Ordering::SeqCst) {
        return None;
    }

    let g = unsafe { get_mut() };
    Some(g.percpu.get_mut(&platform::cpu_id()).unwrap())
}
