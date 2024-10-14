use fdt_parser::Fdt;

fn main() {
    let bytes = include_bytes!("../dtb/phytium.dtb");
    let fdt = Fdt::from_bytes(bytes).unwrap();
    for region in fdt.reserved_memory_regions() {
        println!("region: {:?}", region);
    }
}
