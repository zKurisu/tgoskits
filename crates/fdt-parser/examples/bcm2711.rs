use fdt_parser::{Fdt, FdtRef};

fn main() {
    let bytes = include_bytes!("../dtb/bcm2711-rpi-4-b.dtb");
    // let fdt = FdtRef::from_bytes(bytes).unwrap();

    // println!("fdt size: {}", fdt.total_size());

    // let root = fdt.all_nodes().next().unwrap();

    // for prop in root.properties() {
    //     println!("   prop: {}", prop.name);
    // }

    // for node in fdt.all_nodes() {
    //     println!("node: {}  {}", node.level, node.name);
    //     for prop in node.properties(){
    //         println!("   prop: {}" , prop.name);
    //     }
    // }

    let fdt = Fdt::from_bytes(bytes).unwrap();
    for region in fdt.reserved_memory_regions() {
        println!("region: {:?}", region);
    }
}
