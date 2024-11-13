use core::ptr::NonNull;

use driver_interface::Register;
use log::*;
use mmu::BootTableConfig;
use spin_on::spin_on;

use crate::{
    driver::{self, device_tree::set_dtb_addr},
    logger::KLogger,
    mem::{self, *},
    platform::{self, app_main},
    println,
    stdout::{self, EarlyDebugWrite},
};

/// 初始化日志和内存
///
/// # Safety
///
/// 1. BSS 应当清零。
/// 2. 若有MMU，应当已开启，且虚拟地址与代码段映射一致。
pub unsafe fn init_log_and_memory(kconfig: &KernelConfig) {
    set_dtb_addr(kconfig.dtb_addr);
    let _ = log::set_logger(&KLogger);
    log::set_max_level(LevelFilter::Trace);
    stdout::set_stdout(EarlyDebugWrite {});
    info!("Logger initialized.");

    mem::init(kconfig);

    let version = env!("CARGO_PKG_VERSION");
    println!("Welcome to sparreal\nVersion: {version}");
    platform::print_system_info();
}

/// 注册驱动
pub fn driver_register_append(registers: impl IntoIterator<Item = Register>) {
    driver::register_append(registers);
}

/// 运行内核主逻辑
///
/// # Safety
///
/// 需在 [init_log_and_memory] 之后执行，[run] 之前可用 [driver_register_append] 注册驱动。
pub unsafe fn run() -> ! {
    spin_on(async {
        driver::init().await;
    });
    platform::irqs_enable();
    app_main();
    println!("Waiting for interrupt...");
    loop {
        platform::wait_for_interrupt();
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryRange {
    pub start: Phys<u8>,
    pub size: usize,
}

impl MemoryRange {
    pub const fn new() -> Self {
        Self {
            start: Phys::new(),
            size: 0,
        }
    }
}

#[repr(C)]
/// 内核配置
#[derive(Clone)]
pub struct KernelConfig {
    /// 启动配置
    pub boot_info: BootTableConfig,
    /// 栈顶
    pub stack_top: Phys<u8>,
    /// 设备树地址
    pub dtb_addr: Option<NonNull<u8>>,
}
