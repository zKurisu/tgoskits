use fdt_parser::Fdt;

fn main() {
    let bytes = include_bytes!("../dtb/phytium.dtb");
    let fdt = Fdt::from_bytes(bytes).unwrap();
    println!("version: {}", fdt.version());
    for region in fdt.reserved_memory_regions() {
        println!("region: {:?}", region);
    }

    for node in fdt.all_nodes() {
        let space = " ".repeat(node.level * 4);
        println!("{}{}", space, node.name());
        for prop in node.propertys() {
            println!("{} - {}", space, prop.name);
        }
    }
}
