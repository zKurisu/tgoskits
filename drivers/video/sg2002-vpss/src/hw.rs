//! Task-context VPSS programming and timeout recovery.

use alloc::sync::Arc;
use core::sync::atomic::{Ordering, fence};

use crate::{
    irq::{CompletionState, IrqHandler, RunState},
    registers::*,
    types::{
        DestinationFrame, Error, Job, Nv12Frame, Rect, RgbColor, RgbPlanarFrame, Size, SourceFrame,
        Yuv422PlanarFrame,
    },
};

const TOP_FORCE_CLOCK: u32 = 1 << 31;
const TOP_IP_TRIGGER: u32 = 1 << 3;
const TOP_SC_V1_ENABLE: u32 = 1 << 1;
const TOP_DEBUG_ENABLE: u32 = 1 << 12;
const TOP_QOS_ENABLE: u32 = 0xff << 16;
const TOP_IMG_D_SELECT: u32 = 1 << 5;
const IMG_FORCE_CLOCK: u32 = 1 << 31;
const IMG_CSC_ENABLE: u32 = 1 << 12;
const IMG_SOURCE_MEMORY: u32 = 2;
const NV12_FORMAT: u32 = 8;
const YUV422_PLANAR_FORMAT: u32 = 1;
const RGB_PLANAR_FORMAT: u32 = 2;
const DEFAULT_BURST: u32 = 7;
const SC_FORCE_CLOCK: u32 = 1 << 31;
const SC_GOP_BYPASS: u32 = 1 << 2;
const SC_CIRCLE_BYPASS: u32 = 1 << 5;
const IMG_RESET_W1T: u32 = 1 << 18;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Diagnostics {
    pub img_debug: u32,
    pub img_axi_status: u32,
    pub scaler_status: u32,
    pub odma_debug: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobCompletion {
    pub sequence: u64,
    pub irq_status: u32,
    pub diagnostics: Diagnostics,
    pub result: Result<(), Error>,
}

/// Serialized task-side owner of the VPSS control and data-path registers.
pub struct VpssControl<I: RegisterIo> {
    io: I,
    completion: Arc<CompletionState>,
    initialized: bool,
}

impl<I: RegisterIo> VpssControl<I> {
    pub fn new(io: I, completion: Arc<CompletionState>) -> Self {
        Self {
            io,
            completion,
            initialized: false,
        }
    }

    pub fn irq_handler(&self) -> IrqHandler<I> {
        IrqHandler::new(self.io.clone(), Arc::clone(&self.completion))
    }

    pub fn completion_state(&self) -> &Arc<CompletionState> {
        &self.completion
    }

    /// 直接读 `TOP_INTR_STATUS`（诊断用）。
    ///
    /// 用途：OS 侧等待超时时，用它区分两种失败——硬件其实已经置位、只是中断
    /// 没送到 CPU（status != 0），还是 VPSS 根本没跑完（status == 0）。
    /// 注意这是"观察"接口，不清 W1C 位，不要拿它当 IRQ handler 的替代品。
    pub fn interrupt_status(&self) -> u32 {
        self.io.read32(TOP_INTR_STATUS)
    }

    /// Initializes only resources owned by IMG_V and SC_V1.
    pub fn initialize(&mut self) {
        self.disable_offline_interrupts();
        self.clear_stale_interrupts();

        self.io.update32(
            TOP_CFG0,
            TOP_FORCE_CLOCK | TOP_IP_TRIGGER,
            TOP_FORCE_CLOCK | TOP_IP_TRIGGER,
        );
        self.io.update32(
            TOP_CFG1,
            TOP_SC_V1_ENABLE | TOP_DEBUG_ENABLE | TOP_QOS_ENABLE,
            TOP_DEBUG_ENABLE | TOP_QOS_ENABLE,
        );
        self.io
            .update32(TOP_AXI, TOP_IMG_D_SELECT, TOP_IMG_D_SELECT);
        // IMG_V uses software trigger: clear bits 13 and 9.
        self.io.update32(TOP_IMG_CTRL, (1 << 13) | (1 << 9), 0);

        // Read the working register banks and use raw-register mode, matching
        // the CV181x vendor implementation.
        self.io.write32(IMG_SHD, 4);
        self.io.write32(SC_SHD, 2);
        self.io.update32(TOP_SHD, 1 << 9, 1 << 9);

        self.program_bilinear_coefficients();
        self.io.update32(
            SC_CFG,
            0x8000_00f7,
            SC_FORCE_CLOCK | SC_GOP_BYPASS | SC_CIRCLE_BYPASS,
        );
        self.top_register_done_and_force_update();
        self.initialized = true;
    }

    pub fn start(&mut self, job: Job) -> Result<(), Error> {
        self.start_with_hook(job, || {})
    }

    /// Programs and starts one scaler job, invoking `before_trigger`
    /// immediately before the MMIO write that starts IMG_V. OS glue uses the
    /// hook to capture a hardware-start timestamp without making this driver
    /// depend on a particular clock implementation.
    pub fn start_with_hook<F>(&mut self, job: Job, before_trigger: F) -> Result<(), Error>
    where
        F: FnOnce(),
    {
        if !self.initialized {
            return Err(Error::NotInitialized);
        }
        job.validate()?;
        self.completion.begin(job.sequence)?;

        self.disable_offline_interrupts();
        self.clear_stale_interrupts();
        self.program_input(job.source);
        self.program_scaler(
            job.source.size(),
            job.crop,
            Size {
                width: job.content.width,
                height: job.content.height,
            },
        );
        self.program_output(job.destination, job.content, job.border_color);

        self.io
            .update32(TOP_INTR_MASK, IRQ_OFFLINE_MASK, IRQ_OFFLINE_MASK);
        self.io
            .update32(TOP_INTR_ENABLE, IRQ_OFFLINE_MASK, IRQ_OFFLINE_MASK);
        self.io
            .update32(TOP_CFG1, TOP_SC_V1_ENABLE, TOP_SC_V1_ENABLE);
        self.top_register_done_and_force_update();

        // Publish all register writes before issuing the hardware start.
        fence(Ordering::SeqCst);
        self.io.update32(IMG_DBG, IMG_RESET_W1T, IMG_RESET_W1T);
        // IMG_V start is bit 1 after the vendor driver's instance inversion.
        before_trigger();
        self.io.update32(TOP_IMG_CTRL, 0x3, 1 << 1);
        Ok(())
    }

    /// Quiesces a completed job and returns stable task-side diagnostics.
    pub fn finish(&mut self) -> Result<JobCompletion, Error> {
        let state = self.completion.state();
        if !matches!(state, RunState::Done | RunState::ProgramLate) {
            return Err(Error::BadState);
        }
        let result = if state == RunState::Done {
            Ok(())
        } else {
            Err(Error::ProgramLate)
        };
        let completion = JobCompletion {
            sequence: self.completion.sequence(),
            irq_status: self.completion.irq_status(),
            diagnostics: self.diagnostics(),
            result,
        };
        self.quiesce();
        if state == RunState::ProgramLate {
            self.reset_blocks();
        }
        self.completion.reset_idle();
        Ok(completion)
    }

    /// Masks device interrupts before per-block reset and status clearing.
    pub fn recover_timeout(&mut self) -> Result<JobCompletion, Error> {
        self.disable_offline_interrupts();
        // Resolve the timeout-vs-final-IRQ race atomically. If the IRQ won,
        // return the real hardware result instead of reporting a false timeout.
        if !self.completion.claim_timeout() {
            return self.finish();
        }
        self.quiesce();
        let diagnostics = self.diagnostics();
        self.reset_blocks();
        self.clear_stale_interrupts();
        self.completion.record_timeout();
        let completion = JobCompletion {
            sequence: self.completion.sequence(),
            irq_status: self.completion.irq_status(),
            diagnostics,
            result: Err(Error::Timeout),
        };
        self.completion.reset_idle();
        Ok(completion)
    }

    pub fn diagnostics(&self) -> Diagnostics {
        Diagnostics {
            img_debug: self.io.read32(IMG_DBG),
            img_axi_status: self.io.read32(IMG_AXI_STATUS),
            scaler_status: self.io.read32(SC_STATUS),
            odma_debug: self.io.read32(ODMA_DBG),
        }
    }

    fn program_input(&self, frame: SourceFrame) {
        match frame {
            SourceFrame::Nv12(frame) => self.program_nv12_input(frame),
            SourceFrame::Yuv422Planar(frame) => self.program_yuv422_planar_input(frame),
        }
    }

    fn program_nv12_input(&self, frame: Nv12Frame) {
        let prefetch = ((2 * (frame.size.width + 1)) / 16).saturating_sub(1);
        let read_threshold = prefetch.min(32);
        let program_threshold = read_threshold / 2;
        self.io.write32(
            IMG_FIFO_THR,
            read_threshold
                | (program_threshold << 8)
                | (read_threshold << 16)
                | (program_threshold << 24),
        );
        self.io.write32(
            IMG_CFG,
            IMG_FORCE_CLOCK
                | IMG_CSC_ENABLE
                | (DEFAULT_BURST << 8)
                | (NV12_FORMAT << 4)
                | IMG_SOURCE_MEMORY,
        );
        self.io.write32(IMG_OFFSET, 0);
        self.io.write32(
            IMG_SIZE,
            ((frame.size.height - 1) << 16) | (frame.size.width - 1),
        );
        self.io.write32(IMG_PITCH_Y, frame.y.stride);
        self.io.write32(IMG_PITCH_C, frame.uv.stride);
        self.write_input_addresses(frame.y.address, frame.uv.address, 0);
        self.program_input_bt601_limited_csc();
    }

    fn program_yuv422_planar_input(&self, frame: Yuv422PlanarFrame) {
        self.io.write32(
            IMG_CFG,
            IMG_FORCE_CLOCK
                | IMG_CSC_ENABLE
                | (DEFAULT_BURST << 8)
                | (YUV422_PLANAR_FORMAT << 4)
                | IMG_SOURCE_MEMORY,
        );
        self.io.write32(IMG_OFFSET, 0);
        self.io.write32(
            IMG_SIZE,
            ((frame.size.height - 1) << 16) | (frame.size.width - 1),
        );
        self.io.write32(IMG_PITCH_Y, frame.y.stride);
        self.io.write32(IMG_PITCH_C, frame.cb.stride);
        self.write_input_addresses(frame.y.address, frame.cb.address, frame.cr.address);
        self.program_input_bt601_limited_csc();
    }

    fn program_scaler(&self, source: Size, crop: Rect, destination: Size) {
        self.io.write32(
            SC_SRC_SIZE,
            ((source.height - 1) << 12) | (source.width - 1),
        );
        self.io.write32(SC_CROP_OFFSET, (crop.y << 12) | crop.x);
        self.io
            .write32(SC_CROP_SIZE, ((crop.height - 1) << 16) | (crop.width - 1));
        self.io.write32(
            SC_OUT_SIZE,
            ((destination.height - 1) << 16) | (destination.width - 1),
        );

        let h_factor =
            (((crop.width - 1) << 13) + (destination.width >> 1)) / (destination.width - 1).max(1);
        let v_factor = (((crop.height - 1) << 13) + (destination.height >> 1))
            / (destination.height - 1).max(1);
        self.io.update32(SC_H_CFG, 0x03ff_ff00, h_factor << 8);
        self.io.update32(SC_V_CFG, 0x03ff_ff00, v_factor << 8);

        let over_four = (crop.width - 1) / (destination.width - 1).max(1) >= 4
            || (crop.height - 1) / (destination.height - 1).max(1) >= 4;
        self.io
            .write32(SC_SC_CFG, if over_four { 0x13 } else { 0x03 });
        self.io.write32(SC_INITIAL_PHASE, 0);
    }

    fn program_output(&self, frame: DestinationFrame, content: Rect, border_color: RgbColor) {
        match frame {
            DestinationFrame::Nv12(frame) => self.program_nv12_output(frame),
            DestinationFrame::RgbPlanar(frame) => {
                self.program_rgb_planar_output(frame, content, border_color)
            }
        }
    }

    fn program_nv12_output(&self, frame: Nv12Frame) {
        self.io.write32(SC_BORDER_CFG, 0);
        self.io.write32(SC_BORDER_OFFSET, 0);
        self.io.write32(ODMA_CFG, NV12_FORMAT << 8);
        self.io.write32(ODMA_OFFSET_X, 0);
        self.io.write32(ODMA_OFFSET_Y, 0);
        self.io.write32(ODMA_WIDTH, frame.size.width - 1);
        self.io.write32(ODMA_HEIGHT, frame.size.height - 1);
        self.io.write32(ODMA_PITCH_Y, frame.y.stride);
        self.io.write32(ODMA_PITCH_C, frame.uv.stride);
        self.write_output_addresses(frame.y.address, frame.uv.address, 0);
        self.program_output_bt601_limited_csc();
    }

    fn program_rgb_planar_output(
        &self,
        frame: RgbPlanarFrame,
        content: Rect,
        border_color: RgbColor,
    ) {
        let has_border = content.x != 0
            || content.y != 0
            || content.width != frame.size.width
            || content.height != frame.size.height;
        let border_cfg = if has_border {
            (1 << 31)
                | u32::from(border_color.r)
                | (u32::from(border_color.g) << 8)
                | (u32::from(border_color.b) << 16)
        } else {
            0
        };
        self.io.write32(SC_BORDER_CFG, border_cfg);
        self.io
            .write32(SC_BORDER_OFFSET, (content.y << 16) | content.x);
        self.io.write32(ODMA_CFG, RGB_PLANAR_FORMAT << 8);
        self.io.write32(ODMA_OFFSET_X, 0);
        self.io.write32(ODMA_OFFSET_Y, 0);
        self.io.write32(ODMA_WIDTH, frame.size.width - 1);
        self.io.write32(ODMA_HEIGHT, frame.size.height - 1);
        self.io.write32(ODMA_PITCH_Y, frame.r.stride);
        self.io.write32(ODMA_PITCH_C, frame.g.stride);
        self.write_output_addresses(frame.r.address, frame.g.address, frame.b.address);
        // RGB is already produced by IMG_V's input CSC. Output CSC is needed
        // only when converting that RGB stream back to an output YUV format.
        self.io.update32(OUT_CSC_ENABLE, 0x0100_0013, 0);
        self.io.update32(OUT_CSC_ENABLE, 0x0000_0fec, 0);
    }

    fn program_input_bt601_limited_csc(&self) {
        self.io.write32(IMG_CSC_COEF0, 1024);
        self.io.write32(IMG_CSC_COEF1, 1436);
        self.io.write32(IMG_CSC_COEF2, ((8192 | 352) << 16) | 1024);
        self.io.write32(IMG_CSC_COEF3, 8192 | 731);
        self.io.write32(IMG_CSC_COEF4, (1815 << 16) | 1024);
        self.io.write32(IMG_CSC_COEF5, 0);
        self.io.write32(IMG_CSC_SUB, (128 << 16) | (128 << 8));
        self.io.write32(IMG_CSC_ADD, 0);
    }

    fn program_output_bt601_limited_csc(&self) {
        self.io.update32(OUT_CSC_ENABLE, 0x0100_0013, 1);
        self.io.update32(OUT_CSC_ENABLE, 0x0000_0fec, 1 << 8);
        self.io.write32(OUT_CSC_COEF0, (601 << 16) | 306);
        self.io.write32(OUT_CSC_COEF1, ((8192 | 173) << 16) | 117);
        self.io.write32(OUT_CSC_COEF2, (512 << 16) | (8192 | 339));
        self.io.write32(OUT_CSC_COEF3, ((8192 | 429) << 16) | 512);
        self.io.write32(OUT_CSC_COEF4, 8192 | 83);
        self.io.write32(OUT_CSC_OFFSET, (128 << 16) | (128 << 8));
        self.io.write32(OUT_CSC_FRAC0, 0);
        self.io.write32(OUT_CSC_FRAC1, 0);
    }

    fn program_bilinear_coefficients(&self) {
        let mut coefficient_1 = 1024_u32;
        let mut coefficient_2 = 0_u32;
        for phase in 0..128_u32 {
            coefficient_1 -= 4;
            coefficient_2 += 4;
            self.io.write32(SC_COEF1, coefficient_1 << 16);
            self.io.write32(SC_COEF2, coefficient_2 & 0x0fff);
            self.io.write32(SC_COEF0, (0x5 << 8) | phase);
        }
    }

    fn write_input_addresses(&self, y: u64, uv: u64, v: u64) {
        write_address(&self.io, IMG_ADDR0_L, IMG_ADDR0_H, y);
        write_address(&self.io, IMG_ADDR1_L, IMG_ADDR1_H, uv);
        write_address(&self.io, IMG_ADDR2_L, IMG_ADDR2_H, v);
    }

    fn write_output_addresses(&self, y: u64, uv: u64, v: u64) {
        write_address(&self.io, ODMA_ADDR0_L, ODMA_ADDR0_H, y);
        write_address(&self.io, ODMA_ADDR1_L, ODMA_ADDR1_H, uv);
        write_address(&self.io, ODMA_ADDR2_L, ODMA_ADDR2_H, v);
    }

    fn top_register_done_and_force_update(&self) {
        self.io.update32(TOP_CFG0, 1, 1);
        self.io.update32(TOP_SHD, 0xff, 0xff);
    }

    fn disable_offline_interrupts(&self) {
        self.io.update32(TOP_INTR_ENABLE, IRQ_OFFLINE_MASK, 0);
        self.io.update32(TOP_INTR_MASK, IRQ_OFFLINE_MASK, 0);
    }

    fn clear_stale_interrupts(&self) {
        let status = self.io.read32(TOP_INTR_STATUS);
        if status != 0 {
            self.io.write32(TOP_INTR_STATUS, status);
        }
    }

    fn quiesce(&self) {
        self.disable_offline_interrupts();
        self.io.update32(TOP_CFG1, TOP_SC_V1_ENABLE, 0);
        self.top_register_done_and_force_update();
    }

    fn reset_blocks(&self) {
        self.io.update32(IMG_DBG, IMG_RESET_W1T, IMG_RESET_W1T);
        self.io.write32(SC_SHD, 1);
    }
}

fn write_address<I: RegisterIo>(io: &I, low: usize, high: usize, address: u64) {
    io.write32(low, address as u32);
    io.write32(high, (address >> 32) as u32);
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::{
        sync::{Arc as StdArc, Mutex},
        vec,
        vec::Vec,
    };

    use super::*;
    use crate::types::{
        DestinationFrame, Plane, Rect, RgbColor, RgbPlanarFrame, SourceFrame, Yuv422PlanarFrame,
    };

    #[derive(Clone)]
    struct FakeIo {
        registers: StdArc<Mutex<Vec<u32>>>,
        writes: StdArc<Mutex<Vec<(usize, u32)>>>,
    }

    impl FakeIo {
        fn new() -> Self {
            Self {
                registers: StdArc::new(Mutex::new(vec![0; MMIO_MIN_SIZE / 4])),
                writes: StdArc::new(Mutex::new(Vec::new())),
            }
        }

        fn set(&self, offset: usize, value: u32) {
            self.registers.lock().unwrap()[offset / 4] = value;
        }

        fn get(&self, offset: usize) -> u32 {
            self.registers.lock().unwrap()[offset / 4]
        }
    }

    impl RegisterIo for FakeIo {
        fn read32(&self, offset: usize) -> u32 {
            self.get(offset)
        }

        fn write32(&self, offset: usize, value: u32) {
            self.registers.lock().unwrap()[offset / 4] = value;
            self.writes.lock().unwrap().push((offset, value));
        }
    }

    fn job() -> Job {
        Job {
            source: SourceFrame::Nv12(Nv12Frame {
                size: Size {
                    width: 640,
                    height: 480,
                },
                y: Plane {
                    address: 0x12_0000_1000,
                    stride: 640,
                },
                uv: Plane {
                    address: 0x12_0004_c000,
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
                    address: 0x23_0000_0000,
                    stride: 320,
                },
                uv: Plane {
                    address: 0x23_0001_2c00,
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
            sequence: 9,
            timestamp_ns: 456,
        }
    }

    #[test]
    fn programs_first_milestone_path_and_40_bit_addresses() {
        let io = FakeIo::new();
        let state = Arc::new(CompletionState::new());
        let mut control = VpssControl::new(io.clone(), state);
        control.initialize();
        control.start(job()).unwrap();

        assert_eq!(io.get(IMG_CFG), 0x8000_1782);
        assert_eq!(io.get(SC_SRC_SIZE), (479 << 12) | 639);
        assert_eq!(io.get(SC_OUT_SIZE), (239 << 16) | 319);
        assert_eq!(io.get(ODMA_CFG), 0x800);
        assert_eq!(io.get(IMG_ADDR0_H), 0x12);
        assert_eq!(io.get(ODMA_ADDR0_H), 0x23);
        assert_eq!(io.get(TOP_INTR_ENABLE) & IRQ_OFFLINE_MASK, IRQ_OFFLINE_MASK);
        assert_ne!(io.get(TOP_CFG1) & TOP_SC_V1_ENABLE, 0);
    }

    #[test]
    fn programs_official_yuv422_planar_input_format_and_three_addresses() {
        let io = FakeIo::new();
        let state = Arc::new(CompletionState::new());
        let mut control = VpssControl::new(io.clone(), state);
        control.initialize();
        let mut job = job();
        job.source = SourceFrame::Yuv422Planar(Yuv422PlanarFrame {
            size: Size {
                width: 640,
                height: 480,
            },
            y: Plane {
                address: 0x12_0000_1000,
                stride: 640,
            },
            cb: Plane {
                address: 0x12_0004_c000,
                stride: 320,
            },
            cr: Plane {
                address: 0x12_0007_1800,
                stride: 320,
            },
        });
        control.start(job).unwrap();

        assert_eq!(io.get(IMG_CFG), 0x8000_1712);
        assert_eq!(io.get(IMG_PITCH_Y), 640);
        assert_eq!(io.get(IMG_PITCH_C), 320);
        assert_eq!(io.get(IMG_ADDR0_L), 0x0000_1000);
        assert_eq!(io.get(IMG_ADDR1_L), 0x0004_c000);
        assert_eq!(io.get(IMG_ADDR2_L), 0x0007_1800);
        assert_eq!(io.get(IMG_ADDR2_H), 0x12);
    }

    #[test]
    fn programs_rgb_planar_letterbox_without_output_csc() {
        let io = FakeIo::new();
        let state = Arc::new(CompletionState::new());
        let mut control = VpssControl::new(io.clone(), state);
        control.initialize();
        io.set(OUT_CSC_ENABLE, u32::MAX);
        let mut job = job();
        job.destination = DestinationFrame::RgbPlanar(RgbPlanarFrame {
            size: Size {
                width: 640,
                height: 640,
            },
            r: Plane {
                address: 0x23_0000_0000,
                stride: 640,
            },
            g: Plane {
                address: 0x23_0006_4000,
                stride: 640,
            },
            b: Plane {
                address: 0x23_000c_8000,
                stride: 640,
            },
        });
        job.content = Rect {
            x: 0,
            y: 80,
            width: 640,
            height: 480,
        };
        job.border_color = RgbColor::default();
        control.start(job).unwrap();

        assert_eq!(io.get(SC_OUT_SIZE), (479 << 16) | 639);
        assert_eq!(io.get(SC_BORDER_CFG), 1 << 31);
        assert_eq!(io.get(SC_BORDER_OFFSET), 80 << 16);
        assert_eq!(io.get(ODMA_CFG), RGB_PLANAR_FORMAT << 8);
        assert_eq!(io.get(ODMA_WIDTH), 639);
        assert_eq!(io.get(ODMA_HEIGHT), 639);
        assert_eq!(io.get(ODMA_ADDR0_L), 0);
        assert_eq!(io.get(ODMA_ADDR1_L), 0x0006_4000);
        assert_eq!(io.get(ODMA_ADDR2_L), 0x000c_8000);
        assert_eq!(io.get(OUT_CSC_ENABLE) & 0x0100_0fff, 0);
    }

    #[test]
    fn irq_reads_once_and_w1c_clears_exact_snapshot() {
        let io = FakeIo::new();
        let state = Arc::new(CompletionState::new());
        state.begin(42).unwrap();
        io.set(TOP_INTR_STATUS, IRQ_IMG_V_END | IRQ_SC_V1_END);
        let handler = IrqHandler::new(io.clone(), Arc::clone(&state));
        let event = handler.handle().unwrap();

        assert!(event.wake_waiter);
        assert_eq!(state.state(), RunState::Done);
        assert!(
            io.writes
                .lock()
                .unwrap()
                .contains(&(TOP_INTR_STATUS, IRQ_IMG_V_END | IRQ_SC_V1_END))
        );
    }

    #[test]
    fn program_late_wins_over_done() {
        let io = FakeIo::new();
        let state = Arc::new(CompletionState::new());
        state.begin(42).unwrap();
        io.set(TOP_INTR_STATUS, IRQ_SC_V1_END | IRQ_PROGRAM_LATE);
        let event = IrqHandler::new(io, Arc::clone(&state)).handle().unwrap();

        assert!(event.wake_waiter);
        assert_eq!(state.state(), RunState::ProgramLate);
    }

    #[test]
    fn completion_timestamp_is_reset_for_each_job() {
        let state = CompletionState::new();
        state.begin(1).unwrap();
        state.record_finished_at_ns(123_456);
        assert_eq!(state.finished_at_ns(), 123_456);

        state.reset_idle();
        state.begin(2).unwrap();
        assert_eq!(state.finished_at_ns(), 0);
    }

    #[test]
    fn start_hook_runs_when_hardware_is_triggered() {
        let io = FakeIo::new();
        let state = Arc::new(CompletionState::new());
        let mut control = VpssControl::new(io, state);
        control.initialize();
        let mut hook_calls = 0;

        control.start_with_hook(job(), || hook_calls += 1).unwrap();

        assert_eq!(hook_calls, 1);
    }

    #[test]
    fn timeout_recovery_masks_interrupts_and_resets_blocks() {
        let io = FakeIo::new();
        let state = Arc::new(CompletionState::new());
        let mut control = VpssControl::new(io.clone(), state);
        control.initialize();
        control.start(job()).unwrap();
        let completion = control.recover_timeout().unwrap();

        assert_eq!(completion.result, Err(Error::Timeout));
        assert_eq!(io.get(TOP_INTR_ENABLE) & IRQ_OFFLINE_MASK, 0);
        assert_eq!(io.get(SC_SHD), 1);
    }

    #[test]
    fn final_irq_wins_timeout_race() {
        let io = FakeIo::new();
        let state = Arc::new(CompletionState::new());
        let mut control = VpssControl::new(io.clone(), Arc::clone(&state));
        control.initialize();
        control.start(job()).unwrap();
        io.set(TOP_INTR_STATUS, IRQ_SC_V1_END);
        assert!(control.irq_handler().handle().unwrap().wake_waiter);

        let completion = control.recover_timeout().unwrap();
        assert_eq!(completion.result, Ok(()));
        assert_eq!(state.stats().timeout_errors, 0);
    }
}
