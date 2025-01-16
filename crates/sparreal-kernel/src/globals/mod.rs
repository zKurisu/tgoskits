use core::{cell::UnsafeCell, ops::Range};

use alloc::collections::btree_map::BTreeMap;
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
    percpu: BTreeMap<usize, percpu::PerCPU>,
}

struct LazyGlobal(UnsafeCell<Option<GlobalVal>>);

unsafe impl Sync for LazyGlobal {}

static GLOBAL: LazyGlobal = LazyGlobal::new();

pub fn global_val() -> &'static GlobalVal {
    unsafe { (&*GLOBAL.0.get()).as_ref().unwrap() }
}
impl LazyGlobal {
    const fn new() -> LazyGlobal {
        LazyGlobal(UnsafeCell::new(None))
    }
}

/// # Safty
/// 只能在其他CPU启动前调用
pub(crate) unsafe fn edit(f: impl FnOnce(&mut GlobalVal)) {
    unsafe {
        let global = (&mut *GLOBAL.0.get()).as_mut().unwrap();
        f(global);
    }
}

unsafe fn get_mut() -> &'static mut GlobalVal {
    unsafe { (&mut *GLOBAL.0.get()).as_mut().unwrap() }
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
        GLOBAL.0.get().write(Some(g));

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
}

pub(crate) fn cpu_global() -> &'static PerCPU {
    let g = unsafe { get_mut() };
    g.percpu.get(&platform::cpu_id()).unwrap()
}

pub(crate) unsafe fn cpu_global_mut() -> &'static mut PerCPU {
    let g = unsafe { get_mut() };
    g.percpu.get_mut(&platform::cpu_id()).unwrap()
}
