//! Validated frame types for the offline VPSS path.

use core::fmt;

pub const MIN_DIMENSION: u32 = 32;
pub const MAX_DIMENSION: u32 = 2880;
pub const STRIDE_ALIGNMENT: u32 = 16;
pub const MAX_DMA_ADDRESS: u64 = (1_u64 << 40) - 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    NotInitialized,
    Busy,
    InvalidDimension,
    InvalidCrop,
    InvalidOutputRect,
    InvalidStride,
    InvalidAddress,
    ProgramLate,
    Timeout,
    BadState,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    fn validate(self) -> Result<(), Error> {
        if self.width < MIN_DIMENSION
            || self.height < MIN_DIMENSION
            || self.width > MAX_DIMENSION
            || self.height > MAX_DIMENSION
            || !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
        {
            return Err(Error::InvalidDimension);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn validate(self, source: Size) -> Result<(), Error> {
        Size {
            width: self.width,
            height: self.height,
        }
        .validate()?;
        if !self.x.is_multiple_of(2)
            || !self.y.is_multiple_of(2)
            || self
                .x
                .checked_add(self.width)
                .is_none_or(|v| v > source.width)
            || self
                .y
                .checked_add(self.height)
                .is_none_or(|v| v > source.height)
        {
            return Err(Error::InvalidCrop);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Plane {
    pub address: u64,
    pub stride: u32,
}

impl Plane {
    fn validate(self, width: u32) -> Result<(), Error> {
        if self.address > MAX_DMA_ADDRESS
            || !self.address.is_multiple_of(u64::from(STRIDE_ALIGNMENT))
        {
            return Err(Error::InvalidAddress);
        }
        if self.stride < width
            || self.stride > 0x00ff_ffff
            || !self.stride.is_multiple_of(STRIDE_ALIGNMENT)
        {
            return Err(Error::InvalidStride);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Nv12Frame {
    pub size: Size,
    pub y: Plane,
    pub uv: Plane,
}

impl Nv12Frame {
    pub fn validate(self) -> Result<(), Error> {
        self.size.validate()?;
        self.y.validate(self.size.width)?;
        self.uv.validate(self.size.width)?;
        Ok(())
    }

    /// Bytes touched by the Y plane, relative to its base address.
    pub fn y_span(self) -> Result<u64, Error> {
        plane_span(self.y.stride, self.size.height, self.size.width)
    }

    /// Bytes touched by the interleaved UV plane, relative to its base.
    pub fn uv_span(self) -> Result<u64, Error> {
        plane_span(self.uv.stride, self.size.height / 2, self.size.width)
    }
}

/// Three-plane, horizontally subsampled YUV 4:2:2 input.
///
/// Each chroma plane contains `width / 2` bytes on every image row. This is
/// the layout produced by the CV181x JPU for a 4:2:2 planar decode and matches
/// the vendor VPSS `PIXEL_FORMAT_YUV_PLANAR_422` format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Yuv422PlanarFrame {
    pub size: Size,
    pub y: Plane,
    pub cb: Plane,
    pub cr: Plane,
}

impl Yuv422PlanarFrame {
    pub fn validate(self) -> Result<(), Error> {
        self.size.validate()?;
        self.y.validate(self.size.width)?;
        self.cb.validate(self.size.width / 2)?;
        self.cr.validate(self.size.width / 2)?;
        // IMG_V exposes one chroma-pitch register shared by Cb and Cr.
        if self.cb.stride != self.cr.stride {
            return Err(Error::InvalidStride);
        }
        Ok(())
    }

    pub fn y_span(self) -> Result<u64, Error> {
        plane_span(self.y.stride, self.size.height, self.size.width)
    }

    pub fn cb_span(self) -> Result<u64, Error> {
        plane_span(self.cb.stride, self.size.height, self.size.width / 2)
    }

    pub fn cr_span(self) -> Result<u64, Error> {
        plane_span(self.cr.stride, self.size.height, self.size.width / 2)
    }
}

/// Three independent 8-bit RGB planes as produced by SC_V1 ODMA.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct RgbPlanarFrame {
    pub size: Size,
    pub r: Plane,
    pub g: Plane,
    pub b: Plane,
}

impl RgbPlanarFrame {
    pub fn validate(self) -> Result<(), Error> {
        self.size.validate()?;
        self.r.validate(self.size.width)?;
        self.g.validate(self.size.width)?;
        self.b.validate(self.size.width)?;
        // ODMA exposes one pitch register shared by the second and third plane.
        if self.g.stride != self.b.stride {
            return Err(Error::InvalidStride);
        }
        Ok(())
    }

    pub fn r_span(self) -> Result<u64, Error> {
        plane_span(self.r.stride, self.size.height, self.size.width)
    }

    pub fn g_span(self) -> Result<u64, Error> {
        plane_span(self.g.stride, self.size.height, self.size.width)
    }

    pub fn b_span(self) -> Result<u64, Error> {
        plane_span(self.b.stride, self.size.height, self.size.width)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceFrame {
    Nv12(Nv12Frame),
    Yuv422Planar(Yuv422PlanarFrame),
}

impl SourceFrame {
    pub fn size(self) -> Size {
        match self {
            Self::Nv12(frame) => frame.size,
            Self::Yuv422Planar(frame) => frame.size,
        }
    }

    pub fn validate(self) -> Result<(), Error> {
        match self {
            Self::Nv12(frame) => frame.validate(),
            Self::Yuv422Planar(frame) => frame.validate(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DestinationFrame {
    Nv12(Nv12Frame),
    RgbPlanar(RgbPlanarFrame),
}

impl DestinationFrame {
    pub fn size(self) -> Size {
        match self {
            Self::Nv12(frame) => frame.size,
            Self::RgbPlanar(frame) => frame.size,
        }
    }

    pub fn validate(self) -> Result<(), Error> {
        match self {
            Self::Nv12(frame) => frame.validate(),
            Self::RgbPlanar(frame) => frame.validate(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

fn plane_span(stride: u32, rows: u32, row_bytes: u32) -> Result<u64, Error> {
    u64::from(rows.saturating_sub(1))
        .checked_mul(u64::from(stride))
        .and_then(|v| v.checked_add(u64::from(row_bytes)))
        .ok_or(Error::InvalidStride)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Job {
    pub source: SourceFrame,
    pub crop: Rect,
    pub destination: DestinationFrame,
    /// Scaled image rectangle inside the destination canvas. Any surrounding
    /// area is filled by the scaler border generator.
    pub content: Rect,
    pub border_color: RgbColor,
    /// User-owned sequence propagated unchanged to completion metadata.
    pub sequence: u64,
    /// Source capture timestamp propagated unchanged to completion metadata.
    pub timestamp_ns: u64,
}

impl Job {
    pub fn validate(self) -> Result<(), Error> {
        self.source.validate()?;
        self.destination.validate()?;
        self.crop.validate(self.source.size())?;
        self.content
            .validate(self.destination.size())
            .map_err(|_| Error::InvalidOutputRect)?;
        if matches!(self.destination, DestinationFrame::Nv12(_))
            && (self.content.x != 0
                || self.content.y != 0
                || self.content.width != self.destination.size().width
                || self.content.height != self.destination.size().height)
        {
            return Err(Error::InvalidOutputRect);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_job() -> Job {
        Job {
            source: SourceFrame::Nv12(Nv12Frame {
                size: Size {
                    width: 640,
                    height: 480,
                },
                y: Plane {
                    address: 0x1000,
                    stride: 640,
                },
                uv: Plane {
                    address: 0x4c000,
                    stride: 640,
                },
            }),
            crop: Rect {
                x: 0,
                y: 0,
                width: 640,
                height: 480,
            },
            destination: DestinationFrame::Nv12(Nv12Frame {
                size: Size {
                    width: 320,
                    height: 240,
                },
                y: Plane {
                    address: 0x80_0000,
                    stride: 320,
                },
                uv: Plane {
                    address: 0x81_2c00,
                    stride: 320,
                },
            }),
            content: Rect {
                x: 0,
                y: 0,
                width: 320,
                height: 240,
            },
            border_color: RgbColor::default(),
            sequence: 7,
            timestamp_ns: 123,
        }
    }

    #[test]
    fn validates_first_milestone_job() {
        assert_eq!(valid_job().validate(), Ok(()));
    }

    #[test]
    fn rejects_odd_dimensions_and_bad_stride() {
        let mut job = valid_job();
        job.crop.width = 639;
        assert_eq!(job.validate(), Err(Error::InvalidDimension));
        job = valid_job();
        let DestinationFrame::Nv12(mut destination) = job.destination else {
            unreachable!();
        };
        destination.y.stride = 321;
        job.destination = DestinationFrame::Nv12(destination);
        assert_eq!(job.validate(), Err(Error::InvalidStride));
    }

    #[test]
    fn rejects_address_outside_40_bits() {
        let mut job = valid_job();
        let SourceFrame::Nv12(mut source) = job.source else {
            unreachable!();
        };
        source.y.address = 1_u64 << 40;
        job.source = SourceFrame::Nv12(source);
        assert_eq!(job.validate(), Err(Error::InvalidAddress));
    }

    #[test]
    fn rejects_out_of_bounds_crop() {
        let mut job = valid_job();
        job.crop.x = 16;
        assert_eq!(job.validate(), Err(Error::InvalidCrop));
    }

    #[test]
    fn rejects_dimensions_outside_documented_limits() {
        let mut job = valid_job();
        let DestinationFrame::Nv12(mut destination) = job.destination else {
            unreachable!();
        };
        destination.size.width = 30;
        job.destination = DestinationFrame::Nv12(destination);
        assert_eq!(job.validate(), Err(Error::InvalidDimension));
        job = valid_job();
        let SourceFrame::Nv12(mut source) = job.source else {
            unreachable!();
        };
        source.size.height = 2882;
        job.source = SourceFrame::Nv12(source);
        assert_eq!(job.validate(), Err(Error::InvalidDimension));
    }

    #[test]
    fn accepts_documented_boundaries() {
        let frame = Nv12Frame {
            size: Size {
                width: 2880,
                height: 32,
            },
            y: Plane {
                address: 0,
                stride: 2880,
            },
            uv: Plane {
                address: 0x2_0000,
                stride: 2880,
            },
        };
        assert_eq!(frame.validate(), Ok(()));
    }

    #[test]
    fn validates_jpu_yuv422_planar_layout() {
        let frame = Yuv422PlanarFrame {
            size: Size {
                width: 640,
                height: 480,
            },
            y: Plane {
                address: 0x10_0000,
                stride: 640,
            },
            cb: Plane {
                address: 0x14_b000,
                stride: 320,
            },
            cr: Plane {
                address: 0x17_0800,
                stride: 320,
            },
        };

        assert_eq!(frame.validate(), Ok(()));
        assert_eq!(frame.y_span(), Ok(307_200));
        assert_eq!(frame.cb_span(), Ok(153_600));
        assert_eq!(frame.cr_span(), Ok(153_600));
    }

    #[test]
    fn rejects_yuv422_chroma_stride_smaller_than_half_width() {
        let frame = Yuv422PlanarFrame {
            size: Size {
                width: 640,
                height: 480,
            },
            y: Plane {
                address: 0x10_0000,
                stride: 640,
            },
            cb: Plane {
                address: 0x14_b000,
                stride: 304,
            },
            cr: Plane {
                address: 0x17_0800,
                stride: 320,
            },
        };

        assert_eq!(frame.validate(), Err(Error::InvalidStride));
    }

    #[test]
    fn rejects_yuv422_mismatched_chroma_strides() {
        let frame = Yuv422PlanarFrame {
            size: Size {
                width: 640,
                height: 480,
            },
            y: Plane {
                address: 0x10_0000,
                stride: 640,
            },
            cb: Plane {
                address: 0x14_b000,
                stride: 320,
            },
            cr: Plane {
                address: 0x17_0800,
                stride: 336,
            },
        };

        assert_eq!(frame.validate(), Err(Error::InvalidStride));
    }

    #[test]
    fn validates_rgb_letterbox_destination() {
        let mut job = valid_job();
        job.destination = DestinationFrame::RgbPlanar(RgbPlanarFrame {
            size: Size {
                width: 640,
                height: 640,
            },
            r: Plane {
                address: 0x80_0000,
                stride: 640,
            },
            g: Plane {
                address: 0x86_4000,
                stride: 640,
            },
            b: Plane {
                address: 0x8c_8000,
                stride: 640,
            },
        });
        job.content = Rect {
            x: 0,
            y: 80,
            width: 640,
            height: 480,
        };
        assert_eq!(job.validate(), Ok(()));

        job.content.y = 82;
        job.content.height = 560;
        assert_eq!(job.validate(), Err(Error::InvalidOutputRect));
    }
}
