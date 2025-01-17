use core::{cell::UnsafeCell, error::Error, ptr::NonNull};

use alloc::{boxed::Box, format, string::ToString, sync::Arc, vec::Vec};
use arm_gic_driver::{GicGeneric, GicV3, Trigger};
use sparreal_kernel::{
    driver_interface::{
        self, DriverGeneric, DriverResult, ProbeFn, RegAddress,
        interrupt_controller::{self, CpuId, InterruptControllerPerCpu},
    },
    mem::iomap,
};
use sparreal_macros::module_driver;

use super::*;

module_driver!(
    name: "GICv3",
    compatibles: "arm,gic-v3\n",
    probe: ProbeFn::InterruptController(probe_gic),
);

struct Gic {
    gicd: NonNull<u8>,
    gicr: NonNull<u8>,
    gic: Arc<UnsafeCell<Option<GicV3>>>,
}

unsafe impl Send for Gic {}

impl Gic {
    fn new(gicd: NonNull<u8>, gicr: NonNull<u8>) -> Self {
        Self {
            gic: Arc::new(UnsafeCell::new(None)),
            gicd,
            gicr,
        }
    }
}

struct GicPerCpu(Arc<UnsafeCell<Option<GicV3>>>);

unsafe impl Send for GicPerCpu {}

impl GicPerCpu {
    fn get_mut(&self) -> &mut GicV3 {
        unsafe { &mut *self.0.get() }.as_mut().unwrap()
    }
}

impl InterruptControllerPerCpu for GicPerCpu {
    fn get_and_acknowledge_interrupt(&self) -> Option<interrupt_controller::IrqId> {
        self.get_mut()
            .get_and_acknowledge_interrupt()
            .map(|id| (id.to_u32() as usize).into())
    }

    fn end_interrupt(&self, irq: interrupt_controller::IrqId) {
        self.get_mut().end_interrupt(convert_id(irq));
    }

    fn irq_enable(&self, irq: interrupt_controller::IrqId) {
        self.get_mut().irq_enable(convert_id(irq));
    }

    fn irq_disable(&self, irq: interrupt_controller::IrqId) {
        self.get_mut().irq_disable(convert_id(irq));
    }

    fn set_priority(&self, irq: interrupt_controller::IrqId, priority: usize) {
        self.get_mut().set_priority(convert_id(irq), priority);
    }

    fn set_trigger(
        &self,
        irq: interrupt_controller::IrqId,
        trigger: interrupt_controller::Trigger,
    ) {
        self.get_mut().set_trigger(convert_id(irq), match trigger {
            interrupt_controller::Trigger::EdgeBoth => Trigger::Edge,
            interrupt_controller::Trigger::EdgeRising => Trigger::Edge,
            interrupt_controller::Trigger::EdgeFailling => Trigger::Edge,
            interrupt_controller::Trigger::LevelHigh => Trigger::Level,
            interrupt_controller::Trigger::LevelLow => Trigger::Level,
        });
    }

    fn set_bind_cpu(&self, irq: interrupt_controller::IrqId, cpu_list: &[CpuId]) {
        let id_list = cpu_list
            .iter()
            .map(|v| arm_gic_driver::MPID::from(Into::<usize>::into(*v)))
            .map(|v| v.into())
            .collect::<Vec<_>>();

        self.get_mut().set_bind_cpu(convert_id(irq), &id_list);
    }

    fn parse_fdt_config(&self, prop_interupt: &[usize]) -> Result<IrqConfig, Box<dyn Error>> {
        fdt_parse_irq_config(prop_interupt)
    }
}

impl DriverGeneric for Gic {
    fn name(&self) -> alloc::string::String {
        "GICv3".to_string()
    }

    fn open(&mut self) -> Result<(), alloc::string::String> {
        let gic = GicV3::new(self.gicd, self.gicr).map_err(|e| format!("{:?}", e))?;
        unsafe { &mut *self.gic.get() }.replace(gic);

        Ok(())
    }
}

impl interrupt_controller::InterruptController for Gic {
    fn current_cpu_setup(&self) -> Box<dyn interrupt_controller::InterruptControllerPerCpu> {
        unsafe { &mut *self.gic.get() }
            .as_mut()
            .unwrap()
            .current_cpu_setup();
        Box::new(GicPerCpu(self.gic.clone()))
    }
}

fn probe_gic(regs: Vec<RegAddress>) -> interrupt_controller::Driver {
    let gicd_reg = regs[0];
    let gicc_reg = regs[1];
    let gicd = iomap(gicd_reg.addr.into(), gicd_reg.size.unwrap_or(0x1000));
    let gicr = iomap(gicc_reg.addr.into(), gicc_reg.size.unwrap_or(0x1000));

    Box::new(Gic::new(gicd, gicr))
}
