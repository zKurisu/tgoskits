//! Stable userspace ABI for `/dev/cvi-vpss0`.

use bytemuck::AnyBitPattern;

pub const VPSS_ABI_VERSION: u32 = 1;
pub const VPSS_FEATURE_NV12: u32 = 1 << 0;
pub const VPSS_FEATURE_SCALE: u32 = 1 << 1;
pub const VPSS_FEATURE_BT601_CSC: u32 = 1 << 2;
pub const VPSS_FEATURE_ION_FD: u32 = 1 << 3;
pub const VPSS_FEATURE_IRQ: u32 = 1 << 4;
pub const VPSS_FEATURE_YUV422P_INPUT: u32 = 1 << 5;
pub const VPSS_FEATURE_RGB_PLANAR_OUTPUT: u32 = 1 << 6;
pub const VPSS_FEATURE_BORDER: u32 = 1 << 7;

const IOC_WRITE: u32 = 1;
const IOC_READ: u32 = 2;
const VPSS_IOCTL_TYPE: u8 = b'V';

const fn ioctl(direction: u32, number: u8, size: usize) -> u32 {
    (direction << 30) | ((size as u32) << 16) | ((VPSS_IOCTL_TYPE as u32) << 8) | number as u32
}

pub const VPSS_IOCTL_RUN: u32 = ioctl(IOC_READ | IOC_WRITE, 1, core::mem::size_of::<VpssRun>());
pub const VPSS_IOCTL_GET_INFO: u32 = ioctl(IOC_READ, 2, core::mem::size_of::<VpssInfo>());
pub const VPSS_IOCTL_GET_STATS: u32 = ioctl(IOC_READ, 3, core::mem::size_of::<VpssStats>());
pub const VPSS_IOCTL_RESET_STATS: u32 = ioctl(0, 4, 0);
pub const VPSS_IOCTL_RUN_YUV422P: u32 = ioctl(
    IOC_READ | IOC_WRITE,
    5,
    core::mem::size_of::<VpssRunYuv422p>(),
);
pub const VPSS_IOCTL_RUN_YUV422P_RGB: u32 = ioctl(
    IOC_READ | IOC_WRITE,
    6,
    core::mem::size_of::<VpssRunYuv422pRgb>(),
);

pub const VPSS_STATUS_OK: i32 = 0;
pub const VPSS_STATUS_INVALID: i32 = -22;
pub const VPSS_STATUS_IO: i32 = -5;
pub const VPSS_STATUS_TIMEOUT: i32 = -110;
pub const VPSS_STATUS_BUSY: i32 = -16;

#[derive(Clone, Copy, Debug, Default, AnyBitPattern)]
#[repr(C)]
pub struct VpssRun {
    pub abi_version: u32,
    pub flags: u32,
    pub source_fd: i32,
    pub destination_fd: i32,

    pub source_y_offset: u64,
    pub source_uv_offset: u64,
    pub destination_y_offset: u64,
    pub destination_uv_offset: u64,
    pub sequence: u64,
    pub timestamp_ns: u64,

    pub source_width: u32,
    pub source_height: u32,
    pub source_y_stride: u32,
    pub source_uv_stride: u32,
    pub crop_x: u32,
    pub crop_y: u32,
    pub crop_width: u32,
    pub crop_height: u32,
    pub destination_width: u32,
    pub destination_height: u32,
    pub destination_y_stride: u32,
    pub destination_uv_stride: u32,

    pub timeout_ms: u32,
    pub status: i32,
    pub irq_status: u32,
    pub reserved0: u32,

    pub queue_enter_ns: u64,
    pub hardware_start_ns: u64,
    pub hardware_done_ns: u64,
    pub elapsed_ns: u64,
    pub output_sequence: u64,
    pub output_timestamp_ns: u64,

    pub img_debug: u32,
    pub img_axi_status: u32,
    pub scaler_status: u32,
    pub odma_debug: u32,
    pub reserved: [u32; 4],
}

impl VpssRun {
    pub fn clear_output(&mut self) {
        self.status = VPSS_STATUS_OK;
        self.irq_status = 0;
        self.queue_enter_ns = 0;
        self.hardware_start_ns = 0;
        self.hardware_done_ns = 0;
        self.elapsed_ns = 0;
        self.output_sequence = 0;
        self.output_timestamp_ns = 0;
        self.img_debug = 0;
        self.img_axi_status = 0;
        self.scaler_status = 0;
        self.odma_debug = 0;
        self.reserved0 = 0;
        self.reserved = [0; 4];
    }
}

/// Offline three-plane YUV422P input with NV12 output.
///
/// A separate command preserves the original `VpssRun` layout and makes the
/// source format part of the typed ABI instead of an ambiguous flag.
#[derive(Clone, Copy, Debug, Default, AnyBitPattern)]
#[repr(C)]
pub struct VpssRunYuv422p {
    pub abi_version: u32,
    pub flags: u32,
    pub source_fd: i32,
    pub destination_fd: i32,

    pub source_y_offset: u64,
    pub source_cb_offset: u64,
    pub source_cr_offset: u64,
    pub destination_y_offset: u64,
    pub destination_uv_offset: u64,
    pub sequence: u64,
    pub timestamp_ns: u64,

    pub source_width: u32,
    pub source_height: u32,
    pub source_y_stride: u32,
    pub source_c_stride: u32,
    pub crop_x: u32,
    pub crop_y: u32,
    pub crop_width: u32,
    pub crop_height: u32,
    pub destination_width: u32,
    pub destination_height: u32,
    pub destination_y_stride: u32,
    pub destination_uv_stride: u32,

    pub timeout_ms: u32,
    pub status: i32,
    pub irq_status: u32,
    pub reserved0: u32,

    pub queue_enter_ns: u64,
    pub hardware_start_ns: u64,
    pub hardware_done_ns: u64,
    pub elapsed_ns: u64,
    pub output_sequence: u64,
    pub output_timestamp_ns: u64,

    pub img_debug: u32,
    pub img_axi_status: u32,
    pub scaler_status: u32,
    pub odma_debug: u32,
    pub reserved: [u32; 4],
}

impl VpssRunYuv422p {
    pub fn clear_output(&mut self) {
        self.status = VPSS_STATUS_OK;
        self.irq_status = 0;
        self.queue_enter_ns = 0;
        self.hardware_start_ns = 0;
        self.hardware_done_ns = 0;
        self.elapsed_ns = 0;
        self.output_sequence = 0;
        self.output_timestamp_ns = 0;
        self.img_debug = 0;
        self.img_axi_status = 0;
        self.scaler_status = 0;
        self.odma_debug = 0;
        self.reserved0 = 0;
        self.reserved = [0; 4];
    }
}

/// Offline JPU YUV422P input to RGB-planar output with hardware letterbox.
///
/// `content_*` describes the scaled image inside the destination canvas.
/// `border_rgb` packs R in bits 7:0, G in 15:8, and B in 23:16.
#[derive(Clone, Copy, Debug, Default, AnyBitPattern)]
#[repr(C)]
pub struct VpssRunYuv422pRgb {
    pub abi_version: u32,
    pub flags: u32,
    pub source_fd: i32,
    pub destination_fd: i32,

    pub source_y_offset: u64,
    pub source_cb_offset: u64,
    pub source_cr_offset: u64,
    pub destination_r_offset: u64,
    pub destination_g_offset: u64,
    pub destination_b_offset: u64,
    pub sequence: u64,
    pub timestamp_ns: u64,

    pub source_width: u32,
    pub source_height: u32,
    pub source_y_stride: u32,
    pub source_c_stride: u32,
    pub crop_x: u32,
    pub crop_y: u32,
    pub crop_width: u32,
    pub crop_height: u32,
    pub content_x: u32,
    pub content_y: u32,
    pub content_width: u32,
    pub content_height: u32,
    pub destination_width: u32,
    pub destination_height: u32,
    pub destination_r_stride: u32,
    pub destination_gb_stride: u32,
    pub border_rgb: u32,

    pub timeout_ms: u32,
    pub status: i32,
    pub irq_status: u32,
    pub reserved0: u32,

    pub queue_enter_ns: u64,
    pub hardware_start_ns: u64,
    pub hardware_done_ns: u64,
    pub elapsed_ns: u64,
    pub output_sequence: u64,
    pub output_timestamp_ns: u64,

    pub img_debug: u32,
    pub img_axi_status: u32,
    pub scaler_status: u32,
    pub odma_debug: u32,
    pub reserved: [u32; 4],
}

impl VpssRunYuv422pRgb {
    pub fn clear_output(&mut self) {
        self.status = VPSS_STATUS_OK;
        self.irq_status = 0;
        self.queue_enter_ns = 0;
        self.hardware_start_ns = 0;
        self.hardware_done_ns = 0;
        self.elapsed_ns = 0;
        self.output_sequence = 0;
        self.output_timestamp_ns = 0;
        self.img_debug = 0;
        self.img_axi_status = 0;
        self.scaler_status = 0;
        self.odma_debug = 0;
        self.reserved0 = 0;
        self.reserved = [0; 4];
    }
}

#[derive(Clone, Copy, Debug, Default, AnyBitPattern)]
#[repr(C)]
pub struct VpssInfo {
    pub abi_version: u32,
    pub features: u32,
    pub min_dimension: u32,
    pub max_dimension: u32,
    pub stride_alignment: u32,
    pub reserved0: u32,
    pub mmio_physical: u64,
    pub mmio_size: u64,
    pub irq_domain: u32,
    pub irq_hwirq: u32,
    pub reserved: [u32; 2],
}

#[derive(Clone, Copy, Debug, Default, AnyBitPattern)]
#[repr(C)]
pub struct VpssStats {
    pub irq_count: u64,
    pub completed_jobs: u64,
    pub program_late_errors: u64,
    pub timeout_errors: u64,
    pub spurious_irqs: u64,
    pub last_irq_status: u32,
    pub reserved0: u32,
    pub submitted_jobs: u64,
    pub failed_jobs: u64,
    pub total_elapsed_ns: u64,
    pub last_elapsed_ns: u64,
    pub max_elapsed_ns: u64,
}

const _: () = assert!(core::mem::size_of::<VpssRun>() == 208);
const _: () = assert!(core::mem::size_of::<VpssRunYuv422p>() == 216);
const _: () = assert!(core::mem::size_of::<VpssRunYuv422pRgb>() == 248);
const _: () = assert!(core::mem::size_of::<VpssInfo>() == 56);
const _: () = assert!(core::mem::size_of::<VpssStats>() == 88);
