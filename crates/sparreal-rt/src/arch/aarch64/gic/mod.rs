use core::error::Error;

use alloc::boxed::Box;
use arm_gic_driver::IntId;
use sparreal_kernel::driver_interface::{
    IrqConfig,
    interrupt_controller::{self, Trigger},
};

mod gic_v2;
mod gic_v3;

fn convert_id(irq: interrupt_controller::IrqId) -> IntId {
    let id: usize = irq.into();
    unsafe { IntId::raw(id as u32) }
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

fn fdt_parse_irq_config(itr: &[u32]) -> Result<IrqConfig, Box<dyn Error>> {
    const SPI: u32 = 0;
    const PPI: u32 = 1;

    let num = itr[1];

    let irq_id: u32 = match itr[0] {
        SPI => IntId::spi(num),
        PPI => IntId::ppi(num),
        _ => panic!("Invalid irq type {}", itr[0]),
    }
    .into();

    let flag = TriggerFlag::from_bits_truncate(itr[2] as _);

    let trigger = if flag.contains(TriggerFlag::EDGE_BOTH) {
        Trigger::EdgeBoth
    } else if flag.contains(TriggerFlag::EDGE_RISING) {
        Trigger::EdgeRising
    } else if flag.contains(TriggerFlag::EDGE_FALLING) {
        Trigger::EdgeFailling
    } else if flag.contains(TriggerFlag::LEVEL_HIGH) {
        Trigger::LevelHigh
    } else if flag.contains(TriggerFlag::LEVEL_LOW) {
        Trigger::LevelLow
    } else {
        panic!("Invalid irq type {}", itr[2])
    };

    Ok(IrqConfig {
        irq: (irq_id as usize).into(),
        trigger,
    })
}
