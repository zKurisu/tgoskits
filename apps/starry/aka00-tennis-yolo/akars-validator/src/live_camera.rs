use std::{
    ffi::{c_int, c_ulong},
    fs::{File, OpenOptions},
    io,
    mem::size_of,
    os::fd::AsRawFd,
    path::Path,
};

use crate::camera::{
    CAMERA_FORMAT_YUV400, CAMERA_FORMAT_YUV420_PLANAR, CAMERA_FORMAT_YUV422_PLANAR,
    CAMERA_FORMAT_YUV440_PLANAR, CAMERA_FORMAT_YUV444_PLANAR, CameraFrame, PlanarYuvFrame,
};

const CVI_CAMERA_IOCTL_INIT: c_ulong = 1;
const CVI_CAMERA_IOCTL_START_ASYNC: c_ulong = 6;
const CVI_CAMERA_IOCTL_STOP_ASYNC: c_ulong = 7;
const CVI_CAMERA_IOCTL_GET_LATEST_FRAME: c_ulong = 8;
const CVI_CAMERA_IOCTL_GET_CAPTURE_STATS: c_ulong = 9;
const CVI_CAMERA_IOCTL_RESET_CAPTURE_STATS: c_ulong = 10;
const CVI_CAMERA_IOCTL_GET_LATEST_YUV_FRAME: c_ulong = 11;

const CVI_CAMERA_FORMAT_MJPEG: u8 = 1;
const DEFAULT_FRAME_CAPACITY: usize = 2 * 1024 * 1024;

unsafe extern "C" {
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraCaptureProfile {
    pub capabilities: u32,
    pub attempts: u32,
    pub frame_total_us: u64,
    pub uvc_total_us: u64,
    pub wait_first_packet_us: u64,
    pub usb_transfer_us: u64,
    pub jpeg_assemble_us: u64,
    pub validate_us: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CameraCaptureStats {
    pub capture_calls: u64,
    pub transfer_attempts: u64,
    pub successful_frames: u64,
    pub failed_frames: u64,
    pub retry_attempts: u64,
    pub invalid_frames: u64,
    pub invalid_soi: u64,
    pub invalid_eoi: u64,
    pub invalid_too_small: u64,
    pub usb_errors: u64,
    pub published_frames: u64,
    pub overwritten_frames: u64,
    pub total_frame_us: u64,
    pub max_frame_us: u64,
    pub last_profile: CameraCaptureProfile,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct CameraFrameRequest {
    buffer: u64,
    capacity: u64,
    last_sequence: u64,
    timeout_ms: u32,
    flags: u32,
    sequence: u64,
    timestamp_ns: u64,
    length: u32,
    width: u16,
    height: u16,
    format: u8,
    reserved: [u8; 3],
    profile: CameraCaptureProfile,
}

const _: [(); 56] = [(); size_of::<CameraCaptureProfile>()];
const _: [(); 168] = [(); size_of::<CameraCaptureStats>()];
const _: [(); 120] = [(); size_of::<CameraFrameRequest>()];

#[derive(Clone, Copy, Debug)]
pub struct CameraFrameMeta {
    pub sequence: u64,
    pub timestamp_ns: u64,
    pub profile: CameraCaptureProfile,
}

pub struct LiveCamera {
    file: File,
    sequence: u64,
    started: bool,
    jpeg_capacity: usize,
}

impl LiveCamera {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).open(path)?;
        ioctl_no_arg(&file, CVI_CAMERA_IOCTL_INIT)?;
        ioctl_no_arg(&file, CVI_CAMERA_IOCTL_RESET_CAPTURE_STATS)?;
        ioctl_no_arg(&file, CVI_CAMERA_IOCTL_START_ASYNC)?;
        Ok(Self {
            file,
            sequence: 0,
            started: true,
            jpeg_capacity: DEFAULT_FRAME_CAPACITY,
        })
    }

    pub fn next_mjpeg(
        &mut self,
        frame: &mut CameraFrame,
        timeout_ms: u32,
    ) -> io::Result<CameraFrameMeta> {
        frame.jpeg.clear();
        if frame.jpeg.capacity() < self.jpeg_capacity {
            frame
                .jpeg
                .reserve_exact(self.jpeg_capacity - frame.jpeg.capacity());
        }

        let mut request = CameraFrameRequest {
            buffer: frame.jpeg.as_mut_ptr() as usize as u64,
            capacity: frame.jpeg.capacity() as u64,
            last_sequence: self.sequence,
            timeout_ms,
            ..CameraFrameRequest::default()
        };
        ioctl_ptr(&self.file, CVI_CAMERA_IOCTL_GET_LATEST_FRAME, &mut request)?;

        if request.format != CVI_CAMERA_FORMAT_MJPEG {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("camera returned format {}, expected MJPEG", request.format),
            ));
        }
        let length = request.length as usize;
        if length > frame.jpeg.capacity() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "camera returned a frame larger than the supplied buffer",
            ));
        }

        // SAFETY: the camera ioctl copied exactly `length` initialized bytes
        // into this allocation and the length was checked against capacity.
        unsafe { frame.jpeg.set_len(length) };
        frame.width = u32::from(request.width);
        frame.height = u32::from(request.height);
        self.sequence = request.sequence;

        Ok(CameraFrameMeta {
            sequence: request.sequence,
            timestamp_ns: request.timestamp_ns,
            profile: request.profile,
        })
    }

    pub fn next_yuv(
        &mut self,
        frame: &mut PlanarYuvFrame,
        timeout_ms: u32,
    ) -> io::Result<CameraFrameMeta> {
        frame.data.clear();
        if frame.data.capacity() < DEFAULT_FRAME_CAPACITY {
            frame
                .data
                .reserve_exact(DEFAULT_FRAME_CAPACITY - frame.data.capacity());
        }

        let mut request = CameraFrameRequest {
            buffer: frame.data.as_mut_ptr() as usize as u64,
            capacity: frame.data.capacity() as u64,
            last_sequence: self.sequence,
            timeout_ms,
            ..CameraFrameRequest::default()
        };
        ioctl_ptr(
            &self.file,
            CVI_CAMERA_IOCTL_GET_LATEST_YUV_FRAME,
            &mut request,
        )?;

        if !is_planar_yuv(request.format) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("camera returned unsupported YUV format {}", request.format),
            ));
        }
        let length = request.length as usize;
        if length > frame.data.capacity() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "camera returned a YUV frame larger than the supplied buffer",
            ));
        }

        // SAFETY: the camera ioctl copied exactly `length` initialized bytes
        // into this allocation and the length was checked against capacity.
        unsafe { frame.data.set_len(length) };
        frame.width = u32::from(request.width);
        frame.height = u32::from(request.height);
        frame.format = request.format;
        self.sequence = request.sequence;

        Ok(CameraFrameMeta {
            sequence: request.sequence,
            timestamp_ns: request.timestamp_ns,
            profile: request.profile,
        })
    }

    pub fn stats(&self) -> io::Result<CameraCaptureStats> {
        let mut stats = CameraCaptureStats::default();
        ioctl_ptr(&self.file, CVI_CAMERA_IOCTL_GET_CAPTURE_STATS, &mut stats)?;
        Ok(stats)
    }

    pub fn stop(&mut self) -> io::Result<()> {
        if !self.started {
            return Ok(());
        }
        ioctl_no_arg(&self.file, CVI_CAMERA_IOCTL_STOP_ASYNC)?;
        self.started = false;
        Ok(())
    }
}

fn is_planar_yuv(format: u8) -> bool {
    matches!(
        format,
        CAMERA_FORMAT_YUV420_PLANAR
            | CAMERA_FORMAT_YUV422_PLANAR
            | CAMERA_FORMAT_YUV440_PLANAR
            | CAMERA_FORMAT_YUV444_PLANAR
            | CAMERA_FORMAT_YUV400
    )
}

impl Drop for LiveCamera {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn ioctl_no_arg(file: &File, request: c_ulong) -> io::Result<()> {
    // SAFETY: the camera ABI defines these requests with no pointer argument.
    let rc = unsafe { ioctl(file.as_raw_fd(), request, 0usize) };
    ioctl_result(rc)
}

fn ioctl_ptr<T>(file: &File, request: c_ulong, value: &mut T) -> io::Result<()> {
    // SAFETY: `value` is a live writable ABI structure for the duration of
    // this blocking ioctl call.
    let rc = unsafe { ioctl(file.as_raw_fd(), request, value as *mut T) };
    ioctl_result(rc)
}

fn ioctl_result(rc: c_int) -> io::Result<()> {
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
