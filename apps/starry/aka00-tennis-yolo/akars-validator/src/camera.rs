pub const CAMERA_FORMAT_YUV420_PLANAR: u8 = 2;
pub const CAMERA_FORMAT_YUV422_PLANAR: u8 = 3;
pub const CAMERA_FORMAT_YUV440_PLANAR: u8 = 4;
pub const CAMERA_FORMAT_YUV444_PLANAR: u8 = 5;
pub const CAMERA_FORMAT_YUV400: u8 = 6;

#[derive(Clone, Debug, Default)]
pub struct CameraFrame {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Default)]
pub struct PlanarYuvFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: u8,
}
