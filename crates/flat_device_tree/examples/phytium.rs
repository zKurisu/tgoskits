extern crate flat_device_tree as fdt;

static MY_FDT: &[u8] = include_bytes!("../dtb/phytium.dtb");

fn main() {
    let fdt = fdt::Fdt::new(MY_FDT).unwrap();


    fdt.memory_reservations().for_each(|r| {
        println!("Reservation @{:p} size {:#x}", r.address(), r.size());
    });

    let memory = fdt.memory().ok().unwrap();
    let primory = memory.regions().next().unwrap();
    let memory_begin = primory.starting_address;

    println!("Primory memory: {:#x}", memory_begin as usize);
    println!("Primory memory size: {:#x}", primory.size.unwrap());

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
