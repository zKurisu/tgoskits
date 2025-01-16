use ansi_rgb::{Foreground, orange};
use log::LevelFilter;

use crate::{
    driver_manager,
    globals::global_val,
    io::{self, print::*},
    logger::KLogger,
    mem::{self, VirtAddr, region, va_offset},
    platform::{app_main, module_registers, platform_name, shutdown},
    platform_if::*,
    println,
};

pub mod debug;

#[cfg(feature = "mmu")]
mod mmu;

#[cfg(feature = "mmu")]
pub use mmu::start;

#[repr(align(0x10))]
fn __start() -> ! {
    early_dbgln("Relocate success.");

    io::print::stdout_use_debug();

    mem::init_heap();

    print_start_msg();

    let _ = log::set_logger(&KLogger);
    log::set_max_level(LevelFilter::Trace);

    PlatformImpl::on_boot_success();

    mem::init_page_and_memory();

    driver_manager::init();

    driver_manager::register_drivers(&module_registers());

    match &global_val().platform_info {
        crate::globals::PlatformInfoKind::DeviceTree(fdt) => {
            driver_manager::init_interrupt_controller_by_fdt(fdt.get_addr()).unwrap();
        }
    }

    app_main();

    shutdown()
}

fn print_info() {
    early_dbg("va: ");
    early_dbg_hexln(va_offset() as _);

    let regions = crate::platform_if::PlatformImpl::kernel_regions();

    early_dbg_mem("kernel.text", regions.text.as_slice());
    early_dbg_mem("kernel.data", regions.data.as_slice());
    early_dbg_mem("kernel.bss ", regions.bss.as_slice());
}

macro_rules! print_pair {
    ($name:expr, $($arg:tt)*) => {
        $crate::print!("{:<30}: {}\r\n", $name, format_args!($($arg)*));
    };
}

fn print_start_msg() {
    println!("{}", LOGO.fg(orange()));

    print_pair!("Version", env!("CARGO_PKG_VERSION"));
    print_pair!("Platfrom", "{}", platform_name());
    print_pair!("Kernel Base", "{:p}", region::text().as_ptr());

    let size =
        region::bss().as_ptr_range().end as usize - region::text().as_ptr_range().start as usize;

    print_pair!("Kernel Size", "{:#}", byte_unit::Byte::from_u64(size as _));
    print_pair!(
        "Kernel Stack Top",
        "{}",
        VirtAddr::from(global_val().kstack_top)
    );

    if let Some(debug) = global_val().platform_info.debugcon() {
        if let Some(c) = debug.compatibles().next() {
            print_pair!("Debug Serial", "{}", c);
        }
    }
}

static LOGO: &'static str = r#"
     _____                                         __
    / ___/ ____   ____ _ _____ _____ ___   ____ _ / /
    \__ \ / __ \ / __ `// ___// ___// _ \ / __ `// / 
   ___/ // /_/ // /_/ // /   / /   /  __// /_/ // /  
  /____// .___/ \__,_//_/   /_/    \___/ \__,_//_/   
       /_/                                           
"#;
