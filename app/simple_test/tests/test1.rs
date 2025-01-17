#![no_std]
#![no_main]
#![feature(used_with_arg)]

#[bare_test::tests]
mod tests {
    use core::hint::spin_loop;

    use bare_test::*;
    use globals::{PlatformInfoKind, global_val};
    use irq::IrqHandleResult;
    use log::{debug, info};

    #[test]
    fn test2() {
        let fdt = match &global_val().platform_info {
            PlatformInfoKind::DeviceTree(fdt) => fdt.get(),
        };

        for node in fdt.find_nodes("/timer") {
            for irq_info in node.irq_info() {
                irq_info
                    .builder(|irq| {
                        debug!("irq: {:?}", irq);

                        IrqHandleResult::Handled
                    })
                    .register();
            }
        }

        assert_eq!(2 + 2, 4)
    }
}
