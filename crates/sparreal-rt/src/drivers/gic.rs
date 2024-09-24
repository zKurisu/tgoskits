use alloc::{boxed::Box, format, string::ToString, vec::Vec};
use arm_gic_driver::*;
use driver_interface::{irq, DriverError, DriverGeneric, Register, RegisterKind};
use futures::{future::LocalBoxFuture, FutureExt};
pub fn register_v2() -> Register {
    Register {
        name: "GICv2".to_string(),
        compatible: ["arm,cortex-a15-gic"].to_vec(),
        kind: RegisterKind::Interupt(Box::new(RegisterGicV2 {})),
    }
}
pub fn register_v3() -> Register {
    Register {
        name: "GICv3".to_string(),
        compatible: ["arm,gic-v3"].to_vec(),
        kind: RegisterKind::Interupt(Box::new(RegisterGicV3 {})),
    }
}
struct RegisterGicV2 {}

impl irq::Register for RegisterGicV2 {
    fn probe<'a>(
        &self,
        config: irq::Config,
    ) -> LocalBoxFuture<'a, driver_interface::DriverResult<irq::BoxDriver>> {
        async move {
            let gic = Gic::new(config.reg1, Config::V2 { gicc: config.reg2 })
                .map_err(|e| DriverError::Init(format!("{:?}", e)))?;
            let b: irq::BoxDriver = Box::new(DriverGic(gic));
            Ok(b)
        }
        .boxed_local()
    }
}
struct RegisterGicV3 {}

impl irq::Register for RegisterGicV3 {
    fn probe<'a>(
        &self,
        config: irq::Config,
    ) -> LocalBoxFuture<'a, driver_interface::DriverResult<irq::BoxDriver>> {
        async move {
            let gic = Gic::new(config.reg1, Config::V3 { gicr: config.reg2 })
                .map_err(|e| DriverError::Init(format!("{:?}", e)))?;

            let b: irq::BoxDriver = Box::new(DriverGic(gic));
            Ok(b)
        }
        .boxed_local()
    }
}
struct DriverGic(Gic);
impl DriverGeneric for DriverGic {}
impl irq::Driver for DriverGic {
    fn get_and_acknowledge_interrupt(&self) -> Option<usize> {
        self.0.get_and_acknowledge_interrupt().map(|id| {
            let id: u32 = id.into();
            id as _
        })
    }

    fn end_interrupt(&self, irq_id: usize) {
        self.0.end_interrupt(unsafe { IntId::raw(irq_id as _) });
    }

    fn irq_max_size(&self) -> usize {
        self.0.irq_max()
    }

    fn enable_irq(&mut self, config: irq::IrqConfig) {
        self.0.irq_enable(IrqConfig {
            intid: unsafe { IntId::raw(config.irq_id as _) },
            trigger: match config.trigger {
                irq::Trigger::Edge => Trigger::Edge,
                irq::Trigger::Level => Trigger::Level,
            },
            priority: config.priority as _,
            cpu_list: &[CPUTarget::CORE0],
        });
    }

    fn disable_irq(&mut self, irq_id: usize) {
        self.0.irq_disable(unsafe { IntId::raw(irq_id as _) });
    }

    fn current_cpu_setup(&self) {
        self.0.current_cpu_setup();
    }

    #[allow(unused)]
    fn fdt_itr_to_config(&self, itr: &[usize]) -> irq::IrqConfig {
        const SPI: usize = 0;
        const PPI: usize = 1;

        const TYPE_NONE: usize = 0;
        const TYPE_EDGE_RISING: usize = 1;
        const TYPE_EDGE_FALLING: usize = 2;
        const TYPE_EDGE_BOTH: usize = TYPE_EDGE_FALLING | TYPE_EDGE_RISING;
        const TYPE_LEVEL_HIGH: usize = 4;
        const TYPE_LEVEL_LOW: usize = 8;

        let num = itr[1] as u32;

        let irq_id: u32 = match itr[0] {
            SPI => IntId::spi(num),
            PPI => IntId::ppi(num),
            _ => panic!("Invalid irq type {}", itr[0]),
        }
        .into();

        let trigger = match itr[2] {
            TYPE_EDGE_RISING => irq::Trigger::Edge,
            TYPE_EDGE_FALLING => irq::Trigger::Edge,
            TYPE_EDGE_BOTH => irq::Trigger::Edge,
            TYPE_LEVEL_HIGH => irq::Trigger::Level,
            TYPE_LEVEL_LOW => irq::Trigger::Level,
            _ => panic!("Invalid irq type {}", itr[2]),
        };

        irq::IrqConfig {
            irq_id: irq_id as _,
            trigger,
            priority: 0,
            cpu_list: Vec::new(),
        }
    }
}
