#![no_std]
#![no_main]
#![feature(used_with_arg)]

#[bare_test::tests]
mod tests {

    use bare_test::*;
    use globals::{PlatformInfoKind, global_val};

    #[test]
    fn test2() {
        let _fdt = match &global_val().platform_info {
            PlatformInfoKind::DeviceTree(fdt) => fdt.get(),
        };

        assert_eq!(2 + 2, 4)
    }
}
