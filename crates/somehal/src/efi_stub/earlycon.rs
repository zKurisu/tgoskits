use acpi::{AcpiError, Handler, PhysicalMapping, sdt::spcr::Spcr};

use super::acpi_handle::AcpiHandle;

pub(crate) fn setup_earlycon() -> Result<(), AcpiError> {
    let tb = crate::acpi::tables(AcpiHandle)?;

    for spsr in tb.find_tables::<Spcr>() {
        println!("Found {:?}", spsr.interface_type());
        println!("  Base address: {:#x?}", spsr.base_address());

        if deal_with_spsr(&spsr).is_some() {
            println!("Early console setup complete.");
            break;
        }
    }

    Ok(())
}

fn deal_with_spsr(spsr: &PhysicalMapping<impl Handler, Spcr>) -> Option<()> {
    println!("Found {:?}", spsr.interface_type());
    let base_addr = match spsr.base_address()? {
        Ok(addr) => addr.address as usize,
        Err(e) => {
            println!("Failed to get base address: {:?}", e);
            return None;
        }
    };
    println!("  Base address: {:#x?}", base_addr);
    println!("  Baud rate: {:?}", spsr.baud_rate());

    Some(())
}
