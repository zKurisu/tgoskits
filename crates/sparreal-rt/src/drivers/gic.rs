use alloc::vec;
use alloc::{boxed::Box, format};
use arm_gic::gicv3::GicV3;
use arm_gic::irq_enable;
use arm_gic_driver::*;
use driver_interface::{
    irq, DriverError, DriverGeneric, DriverKind, DriverResult, DriverSpecific, IrqProbeConfig,
    Probe, ProbeConfig, Register,
};
use futures::{future::LocalBoxFuture, FutureExt};
pub fn register_v2() -> Register {
    Register::new(
        "GICv2",
        vec!["arm,cortex-a15-gic"],
        DriverKind::InteruptChip,
        RegisterGicV2 {},
    )
}
pub fn register_v3() -> Register {
    Register::new(
        "GICv3",
        vec!["arm,gic-v3"],
        DriverKind::InteruptChip,
        RegisterGicV3 {},
    )
}
struct RegisterGicV2 {}

impl Probe for RegisterGicV2 {
    fn probe<'a>(&self, config: ProbeConfig) -> LocalBoxFuture<'a, DriverResult<DriverSpecific>> {
        async move {
            let gic = Gic::new(
                config.reg[0],
                Config::V2 {
                    gicc: config.reg[1],
                },
            )
            .map_err(|e| DriverError::Init(format!("{:?}", e)))?;
            let b: irq::BoxDriver = Box::new(DriverGic(gic));

            Ok(DriverSpecific::InteruptChip(b))
        }
        .boxed_local()
    }
}
struct RegisterGicV3 {}

impl Probe for RegisterGicV3 {
    fn probe<'a>(&self, config: ProbeConfig) -> LocalBoxFuture<'a, DriverResult<DriverSpecific>> {
        async move {
            let gic = Gic::new(
                config.reg[0],
                Config::V3 {
                    gicr: config.reg[1],
                },
            )
            .map_err(|e| DriverError::Init(format!("{:?}", e)))?;

            let b: irq::BoxDriver = Box::new(DriverGic(gic));
            Ok(DriverSpecific::InteruptChip(b))
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

    fn irq_enable(&mut self, config: irq::IrqConfig) {
        self.0.irq_enable(IrqConfig {
            intid: unsafe { IntId::raw(config.irq as _) },
            trigger: match config.trigger {
                irq::Trigger::EdgeRising => Trigger::Edge,
                irq::Trigger::EdgeFailling => Trigger::Edge,
                irq::Trigger::EdgeBoth => Trigger::Edge,
                irq::Trigger::LevelLow => Trigger::Level,
                irq::Trigger::LevelHigh => Trigger::Level,
            },
            priority: config.priority as _,
            cpu_list: &[CPUTarget::CORE0],
        });
    }

    fn irq_enable(&mut self, irq_id: usize) {
        self.0.irq_disable(unsafe { IntId::raw(irq_id as _) });
    }

    fn current_cpu_setup(&self) {
        self.0.current_cpu_setup();
    }

    fn fdt_parse_config(&self, itr: &[usize]) -> IrqProbeConfig {
        fdt_itr_to_config(itr)
    }
}

use bitflags::bitflags;

// The `bitflags!` macro generates `struct`s that manage a set of flags.
bitflags! {
    /// Represents a set of flags.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    struct TriggerFlag: usize {
        const NONE = 0;
        const EDGE_RISING = 1;
        const EDGE_FALLING = 2;
        const EDGE_BOTH = Self::EDGE_RISING.bits()| Self::EDGE_FALLING.bits();
        const LEVEL_HIGH = 4;
        const LEVEL_LOW = 8;
    }
}

fn fdt_itr_to_config(itr: &[usize]) -> IrqProbeConfig {
    const SPI: usize = 0;
    const PPI: usize = 1;

    let num = itr[1] as u32;

    let irq_id: u32 = match itr[0] {
        SPI => IntId::spi(num),
        PPI => IntId::ppi(num),
        _ => panic!("Invalid irq type {}", itr[0]),
    }
    .into();

    let flag = TriggerFlag::from_bits_truncate(itr[2]);

    let trigger = if flag.contains(TriggerFlag::EDGE_BOTH) {
        irq::Trigger::EdgeBoth
    } else if flag.contains(TriggerFlag::EDGE_RISING) {
        irq::Trigger::EdgeRising
    } else if flag.contains(TriggerFlag::EDGE_FALLING) {
        irq::Trigger::EdgeFailling
    } else if flag.contains(TriggerFlag::LEVEL_HIGH) {
        irq::Trigger::LevelHigh
    } else if flag.contains(TriggerFlag::LEVEL_LOW) {
        irq::Trigger::LevelLow
    } else {
        panic!("Invalid irq type {}", itr[2])
    };

    IrqProbeConfig {
        irq: irq_id as _,
        trigger,
    }
}
