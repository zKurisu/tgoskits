use alloc::{boxed::Box, string::ToString, vec::Vec};
use arm_gic_driver::GicGeneric;
use sparreal_kernel::{
    driver_interface::{
        self, DriverGeneric, ProbeFn, RegAddress, interrupt_controller,
    },
    mem::iomap,
};
use sparreal_macros::module_driver;

module_driver!(
    name: "GICv2",
    compatibles: "arm,cortex-a15-gic\n",
    probe: ProbeFn::InterruptController(probe_gic_v2),
);

// #[unsafe(link_section = ".registers")]
// #[unsafe(no_mangle)]
// #[used(linker)]
// pub static DRIVER_GIC_V2: DriverRegister = DriverRegister {
//     name: "GICv2",
//     compatibles: "arm,cortex-a15-gic\n",
//     probe: ProbeFn::InterruptController(probe_gic_v2),
// };

struct GicV2(arm_gic_driver::GicV2);

impl DriverGeneric for GicV2 {
    fn name(&self) -> alloc::string::String {
        "GICv2".to_string()
    }

    fn open(&mut self) -> Result<(), alloc::string::String> {
        Ok(())
    }
}

impl interrupt_controller::InterruptController for GicV2 {
    fn current_cpu_setup(&self) {
        self.0.current_cpu_setup();
    }

    fn get_and_acknowledge_interrupt(
        &self,
    ) -> Option<driver_interface::interrupt_controller::IrqId> {
        todo!()
    }

    fn end_interrupt(&self, irq: driver_interface::interrupt_controller::IrqId) {
        todo!()
    }

    fn irq_max_size(&self) -> usize {
        todo!()
    }

    fn irq_enable(&mut self, irq: driver_interface::interrupt_controller::IrqId) {
        todo!()
    }

    fn irq_disable(&mut self, irq: driver_interface::interrupt_controller::IrqId) {
        todo!()
    }

    fn set_priority(
        &mut self,
        irq: driver_interface::interrupt_controller::IrqId,
        priority: usize,
    ) {
        todo!()
    }

    fn set_trigger(
        &mut self,
        irq: driver_interface::interrupt_controller::IrqId,
        triger: driver_interface::interrupt_controller::Trigger,
    ) {
        todo!()
    }

    fn set_bind_cpu(
        &mut self,
        irq: driver_interface::interrupt_controller::IrqId,
        cpu_list: &[u64],
    ) {
        todo!()
    }

    fn fdt_parse_config(&self, prop_interupt: &[usize]) -> driver_interface::ProbeIrqConfig {
        todo!()
    }
}

fn probe_gic_v2(regs: Vec<RegAddress>) -> interrupt_controller::BoxedDriver {
    let gicd_reg = regs[0];
    let gicc_reg = regs[1];
    let gicd = iomap(gicd_reg.addr.into(), gicd_reg.size.unwrap_or(0x1000));
    let gicc = iomap(gicc_reg.addr.into(), gicc_reg.size.unwrap_or(0x1000));

    Box::new(GicV2(arm_gic_driver::GicV2::new(gicd, gicc).unwrap()))
}
