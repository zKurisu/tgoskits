extern crate flat_device_tree as fdt;

static MY_FDT: &[u8] = include_bytes!("../dtb/phytium-arceos.dtb");

fn main() {
    let fdt = fdt::Fdt::new(MY_FDT).unwrap();

    let chosen = fdt.chosen().unwrap();

    if let Some(stdout) = chosen.stdout() {
        stdout.node().reg_fix().for_each(|r| {
            println!("{:?}", r);
        });
        
        println!(
            "It would write stdout to: {} with params: {:?}",
            stdout.node().name,
            stdout.params()
        );
    }
}


