use core::{cell::UnsafeCell, ops::Range};

use crate::mem::PhysAddr;
pub use crate::platform::PlatformInfoKind;

pub struct GlobalVal {
    pub platform_info: PlatformInfoKind,
    pub kstack_top: PhysAddr,
    pub main_memory: Range<PhysAddr>,
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
