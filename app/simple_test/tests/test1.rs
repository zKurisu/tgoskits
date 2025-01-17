#![no_std]
#![no_main]
#![feature(used_with_arg)]

#[bare_test::tests]
mod tests {
    use bare_test::*;
    use log::info;

    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4)
    }

    #[test]
    fn test2() {
        let fdt = match &global_val().platform_info {
            PlatformInfoKind::DeviceTree(fdt) => fdt.get(),
        };

        let node = fdt.chosen().unwrap().stdout().unwrap();
        let info = node.node.irq_info();

        info!("irq: {:?}", info);

        assert_eq!(2 + 2, 4)
    }
}
