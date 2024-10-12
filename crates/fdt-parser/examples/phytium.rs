use fdt_parser::FdtRef;

fn main() {
    let bytes = include_bytes!("../dtb/phytium.dtb");
    let fdt = FdtRef::from_bytes(bytes).unwrap();

    println!("fdt size: {}", fdt.total_size());
}
