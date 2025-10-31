use core::{fmt::Write, mem};

use uefi::{
    Result,
    boot::MemoryType,
    mem::memory_map::MemoryMap,
    prelude::*,
    proto::{
        console::{gop::GraphicsOutput, text::Output},
        loaded_image::LoadedImage,
        shell_params::ShellParameters,
    },
    system::with_config_table,
    table::cfg::{ACPI_GUID, ACPI2_GUID, ConfigTableEntry},
};
use uefi_raw::table::system::SystemTable;

use crate::{acpi::set_rsdp, arch::relocate};

pub mod pe;

/// EFI PE 入口点 - 符合 EFI ABI 的汇编包装
/// 参数: a0 = image_handle, a1 = system_table
#[unsafe(export_name = "efi_pe_entry")]
#[unsafe(link_section = ".text")]
pub unsafe extern "C" fn efi_pe_entry(
    image_handle: Handle,
    system_table: *const SystemTable,
) -> Status {
    unsafe {
        relocate();
        ::uefi::boot::set_image_handle(image_handle);
        ::uefi::table::set_system_table(system_table);

        crate::console::set_printer(&UefiPrinter);

        if let Err(e) = efi_main() {
            println!("EFI application error: {:?}", e);
            return e.status();
        }

        if let Err(e) = draw_sierpinski() {
            println!("Failed to draw Sierpinski triangle: {:?}", e);
        } else {
            println!("Sierpinski triangle drawn successfully.");
        }

        crate::arch::entry::efi_kernel_prepare();
    }

    // 返回成功状态
    Status::SUCCESS
}

fn efi_main() -> Result {
    find_acpi_rsdp();

    let mem_map = boot::memory_map(MemoryType::LOADER_DATA)?;
    for desc in mem_map.entries() {
        println!("{desc:#x?}");
    }

    let h = boot::get_handle_for_protocol::<LoadedImage>()?;

    let img = boot::open_protocol_exclusive::<LoadedImage>(h)?;

    match img.load_options_as_cstr16() {
        Ok(cmdline) => {
            println!("Kernel command line: {}", cmdline);
            system::with_stdout(|stdout| {
                let _ = cmdline.as_str_in_buf(stdout);
            });
        }
        Err(e) => {
            println!("Failed to get load options as CStr16: {:?}", e);
        }
    }

    Ok(())
}

fn draw_sierpinski() -> Result {
    // Open graphics output protocol.
    let gop_handle = boot::get_handle_for_protocol::<GraphicsOutput>()?;
    let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(gop_handle)?;
    Ok(())
}

struct UefiPrinter;
impl crate::console::Printer for UefiPrinter {
    fn read_byte(&self) -> Option<u8> {
        // system::with_stdin(|stdin| {
        //     let mut buffer = [0u16; 1];
        //     match stdin.read_key(&mut buffer) {
        //         Ok(()) => Some(buffer[0] as u8),
        //         Err(_) => None,
        //     }
        // })
        None
    }

    fn write_str(&self, s: &str) {
        system::with_stdout(|stdout| {
            let _ = stdout.write_str(s);
        });
    }
}

fn find_acpi_rsdp() {
    with_config_table(|config_table| {
        for entry in config_table {
            if entry.guid == ACPI2_GUID {
                // ACPI 2.0 RSDP (推荐)
                println!("Found ACPI 2.0 RSDP at address: {:p}", entry.address);

                set_rsdp(entry.address);
            } else if entry.guid == ACPI_GUID {
                // ACPI 1.0 RSDP (备选)
                println!("Found ACPI 1.0 RSDP at address: {:p}", entry.address);
                set_rsdp(entry.address);
            }
        }
    })
}
