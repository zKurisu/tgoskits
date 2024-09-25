use core::ptr::{slice_from_raw_parts, slice_from_raw_parts_mut, NonNull};

use alloc::{
    string::{String, ToString},
    sync::{Arc, Weak},
    vec::Vec,
};
use device_tree::get_device_tree;
use flat_device_tree::Fdt;
use irq::init_irq;
use log::info;

use crate::{
    stdout::{set_stdout, DriverWrite},
    sync::RwLock,
};

mod container;
pub mod device_tree;
mod irq;
// pub mod manager;
mod uart;

pub use container::*;

pub type DriverArc<T> = Arc<RwLock<T>>;
pub type DriverWeak<T> = Weak<RwLock<T>>;

#[repr(transparent)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DriverId(String);

impl From<&str> for DriverId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct DriverInfo {
    pub id: DriverId,
    pub name: String,
}

#[derive(Clone)]
pub struct DriverUart {
    pub info: DriverInfo,
    pub driver: DriverArc<uart::BoxDriver>,
}

#[derive(Clone)]
pub struct DriverIrqChip {
    pub info: DriverInfo,
    pub driver: DriverArc<irq::BoxDriver>,
}

pub unsafe fn move_dtb(src: *const u8, mut dst: NonNull<u8>) -> Option<&'static [u8]> {
    let fdt = Fdt::from_ptr(src).ok()?;
    let size = fdt.total_size();
    let dest = &mut *slice_from_raw_parts_mut(dst.as_mut(), size);
    let src = &*slice_from_raw_parts(src, size);
    dest.copy_from_slice(src);
    Some(dest)
}

pub async fn init() {
    init_irq().await;
    init_stdout().await;
    info!("Stdout ok!");
}

async fn init_stdout() -> Option<()> {
    let fdt = get_device_tree()?;
    let chosen = fdt.chosen().ok()?;
    let stdout = chosen.stdout()?;
    let node = stdout.node();
    let caps = node.compatible()?.all().collect::<Vec<_>>();

    let register = register_by_compatible(&caps)?;

    probe_by_register(register).await?;

    let driver = uart_by_id(node.name.into())?;

    let stdout = DriverWrite::new(&driver);

    set_stdout(stdout);

    Some(())
}
