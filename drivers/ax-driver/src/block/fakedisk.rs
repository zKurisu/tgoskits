use crate::block::PlatformDeviceBlock;

/// Called explicitly from axruntime after `fs::init()` to avoid confusing
/// the root-device auto-detection (which needs exactly 1 raw block device).
pub fn register(plat_dev: rdrive::PlatformDevice) {
    let disk = fakedisk::FakeDisk::new(512, 32768); // 16 MiB
    plat_dev.register_block(disk);
    log::info!("registered fakedisk: 32768 blocks x 512 bytes = 16 MiB");
}
