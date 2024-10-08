use alloc::{
    boxed::Box,
    format,
    {vec, vec::Vec},
};
use arm_gic_driver::*;
use driver_interface::*;

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
            let gic = GicV2::new(config.reg[0], config.reg[1])
                .map_err(|e| DriverError::Init(format!("{:?}", e)))?;
            let b: irq::BoxDriver = Box::new(DriverGicV2(gic));

            Ok(DriverSpecific::InteruptChip(b))
        }
        .boxed_local()
    }
}
struct RegisterGicV3 {}

impl Probe for RegisterGicV3 {
    fn probe<'a>(&self, config: ProbeConfig) -> LocalBoxFuture<'a, DriverResult<DriverSpecific>> {
        async move {
            let gic = GicV3::new(config.reg[0], config.reg[1])
                .map_err(|e| DriverError::Init(format!("{:?}", e)))?;

            let b: irq::BoxDriver = Box::new(DriverGicV3(gic));
            Ok(DriverSpecific::InteruptChip(b))
        }
        .boxed_local()
    }
}

macro_rules! impl_driver_gic {
    ($name:ident, $inner:ident) => {
        struct $name($inner);
        impl DriverGeneric for $name {}
        impl irq::Driver for $name {
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
                self.0.irq_max_size()
            }

            fn irq_disable(&mut self, irq_id: usize) {
                self.0.irq_disable(unsafe { IntId::raw(irq_id as _) });
            }

            fn current_cpu_setup(&self) {
                self.0.current_cpu_setup();
            }

            fn fdt_parse_config(&self, itr: &[usize]) -> IrqProbeConfig {
                fdt_itr_to_config(itr)
            }

            fn set_priority(&mut self, irq: usize, priority: usize) {
                self.0
                    .set_priority(unsafe { IntId::raw(irq as _) }, priority);
            }

            fn set_trigger(&mut self, irq: usize, trigger: irq::Trigger) {
                self.0.set_trigger(
                    unsafe { IntId::raw(irq as _) },
                    match trigger {
                        irq::Trigger::EdgeBoth => Trigger::Edge,
                        irq::Trigger::EdgeRising => Trigger::Edge,
                        irq::Trigger::EdgeFailling => Trigger::Edge,
                        irq::Trigger::LevelHigh => Trigger::Level,
                        irq::Trigger::LevelLow => Trigger::Level,
                    },
                );
            }

            fn set_bind_cpu(&mut self, irq: usize, cpu_list: &[u64]) {
                let list = cpu_list_to_target_list(cpu_list);
                self.0.set_bind_cpu(unsafe { IntId::raw(irq as _) }, &list);
            }

            fn irq_enable(&mut self, irq: usize) {
                self.0.irq_enable(unsafe { IntId::raw(irq as _) });
            }
        }
    };
}

impl_driver_gic!(DriverGicV2, GicV2);
impl_driver_gic!(DriverGicV3, GicV3);

fn cpu_list_to_target_list(cpu_list: &[u64]) -> Vec<CPUTarget> {
    cpu_list
        .iter()
        .map(|id| cpu_id_u64_to_cpu_id(*id))
        .collect()
}
fn cpu_id_u64_to_cpu_id(cpu_id: u64) -> CPUTarget {
    let mpid: MPID = cpu_id.into();
    mpid.into()
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
