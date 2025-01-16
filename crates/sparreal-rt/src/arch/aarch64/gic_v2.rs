use core::{cell::UnsafeCell, ptr::NonNull};

use alloc::{boxed::Box, format, string::ToString, sync::Arc, vec::Vec};
use arm_gic_driver::{GicGeneric, IntId, Trigger};
use sparreal_kernel::{
    driver_interface::{
        self, DriverGeneric, DruverResult, ProbeFn, RegAddress,
        interrupt_controller::{self, CpuId, InterruptControllerPerCpu},
    },
    mem::iomap,
};
use sparreal_macros::module_driver;

module_driver!(
    name: "GICv2",
    compatibles: "arm,cortex-a15-gic\n",
    probe: ProbeFn::InterruptController(probe_gic_v2),
);

struct GicV2 {
    gicd: NonNull<u8>,
    gicc: NonNull<u8>,
    gic: Arc<UnsafeCell<Option<arm_gic_driver::GicV2>>>,
}

unsafe impl Send for GicV2 {}

impl GicV2 {
    fn new(gicd: NonNull<u8>, gicc: NonNull<u8>) -> Self {
        Self {
            gic: Arc::new(UnsafeCell::new(None)),
            gicd,
            gicc,
        }
    }
}

struct GicV2PerCpu(Arc<UnsafeCell<Option<arm_gic_driver::GicV2>>>);

unsafe impl Send for GicV2PerCpu {}

impl GicV2PerCpu {
    fn get_mut(&self) -> &mut arm_gic_driver::GicV2 {
        unsafe { &mut *self.0.get() }.as_mut().unwrap()
    }
}

impl InterruptControllerPerCpu for GicV2PerCpu {
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

        todo!()
    }
}

fn convert_id(irq: interrupt_controller::IrqId) -> IntId {
    let id: usize = irq.into();
    unsafe { IntId::raw(id as u32) }
}

impl DriverGeneric for GicV2 {
    fn name(&self) -> alloc::string::String {
        "GICv2".to_string()
    }

    fn open(&mut self) -> Result<(), alloc::string::String> {
        let gic =
            arm_gic_driver::GicV2::new(self.gicd, self.gicc).map_err(|e| format!("{:?}", e))?;
        unsafe { &mut *self.gic.get() }.replace(gic);

        Ok(())
    }
}

impl interrupt_controller::InterruptController for GicV2 {
    fn current_cpu_setup(&self) -> Box<dyn interrupt_controller::InterruptControllerPerCpu> {
        unsafe { &mut *self.gic.get() }
            .as_mut()
            .unwrap()
            .current_cpu_setup();
        Box::new(GicV2PerCpu(self.gic.clone()))
    }

    fn parse_fdt_config(
        &self,
        prop_interupt: &[usize],
    ) -> DruverResult<driver_interface::IrqConfig> {
        todo!()
    }
}

fn probe_gic_v2(regs: Vec<RegAddress>) -> interrupt_controller::BoxedDriver {
    let gicd_reg = regs[0];
    let gicc_reg = regs[1];
    let gicd = iomap(gicd_reg.addr.into(), gicd_reg.size.unwrap_or(0x1000));
    let gicc = iomap(gicc_reg.addr.into(), gicc_reg.size.unwrap_or(0x1000));

    Box::new(GicV2::new(gicd, gicc))
}
