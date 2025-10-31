#[unsafe(no_mangle)]
pub unsafe extern "C" fn kernel_entry() -> ! {
    unimplemented!()
}

pub(crate) fn prepare_kernel_entry() {
    println!("Preparing kernel entry...");
}
