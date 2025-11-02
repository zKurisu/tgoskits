use crate::efi_stub::acpi_setup_earlycon;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kernel_entry() -> ! {
    unimplemented!()
}

// cosst

pub(crate) fn efi_setup() {
    if let Err(e) = acpi_setup_earlycon() {
        println!("Failed to setup early console: {e:?}");
    }

    println!("EFI kernel preparation complete.");
}
