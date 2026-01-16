use rdif_intc::Intc;
use rdrive::Device;

mod v3;

fn get_gicd() -> Device<Intc> {
    rdrive::get_one().expect("no interrupt controller found")
}
