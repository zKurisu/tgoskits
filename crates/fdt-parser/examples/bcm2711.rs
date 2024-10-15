use fdt_parser::{Fdt, FdtRef};

fn main() {
    let bytes = include_bytes!("../dtb/bcm2711-rpi-4-b.dtb");


    let fdt = Fdt::from_bytes(bytes).unwrap();
    for region in fdt.reserved_memory_regions() {
        println!("region: {:?}", region);
    }
}
