use alloc::string::String;
use driver_interface::uart;

pub mod device_tree;
pub mod manager;



pub struct Driver{
    pub name: String,
    pub kind: DriverKind,
}


pub enum DriverKind{
    Uart(uart::BoxDriver),
}