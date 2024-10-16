use fdt_parser::Fdt;

fn main() {
    let bytes = include_bytes!("../dtb/bcm2711-rpi-4-b.dtb");

    let fdt = Fdt::from_bytes(bytes).unwrap();
    println!("version: {}", fdt.version());
    for region in fdt.reserved_memory_regions() {
        println!("region: {:?}", region);
    }

    for node in fdt.all_nodes() {
        let space = " ".repeat((node.level - 1) * 4);
        println!("{}{}", space, node.name());

        if node.reg().count() > 0 {

            println!("{} -range: ", space);

            for range in node.ranges() {
                println!("{}     {:?}", space, range);
            }

            println!("{} - reg: ", space);
            for cell in node.reg() {
                println!("{}     {:?}", space, cell);
            }
        }

        // for prop in node.propertys() {
        //     println!("{} - {}", space, prop.name);
        // }
    }
}
