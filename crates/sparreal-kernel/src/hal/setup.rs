use crate::hal::{al, timer};

pub fn start_kernel() -> ! {
    crate::os::logger::init();
    info!("Setting up allocator...");

    crate::os::mem::init_heap(al::memory::memory_map());
    al::platform::post_allocator();
    crate::os::mem::paging::init();
    timer::init();

    if let Some(addr) = al::platform::fdt_addr() {
        info!("Initializing rdrive with FDT at {:?}", addr);
        rdrive::init(rdrive::Platform::Fdt { addr }).unwrap();

        rdrive::register_append(&al::platform::driver_registers());

        rdrive::probe_pre_kernel().unwrap();
        rdrive::probe_all(true).unwrap();
    }

    al::cpu::irq_local_set_enable(true);

    unsafe extern "C" {
        fn __sparreal_main();
    }

    unsafe { __sparreal_main() };

    al::platform::shutdown()
}
