use std::{error::Error, fmt, path::Path};

use crate::{
    camera::{CameraFrame, PlanarYuvFrame},
    detector::Detection,
};

#[derive(Clone, Copy, Debug)]
pub struct InferenceConfig {
    pub classes_num: i32,
    pub confidence_threshold: f32,
    pub iou_threshold: f32,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            classes_num: 1,
            confidence_threshold: 0.5,
            iou_threshold: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct InferTiming {
    /// CPU-side JPEG decode, resize, letterbox, and tensor packing microseconds.
    pub preprocess_us: i64,
    /// CVI_NN_Forward microseconds.
    pub forward_us: i64,
    /// detection parse + dequant + NMS + box correction microseconds.
    pub postprocess_us: i64,
}

/// Runtime-observed input tensor metadata used to guard the VPSS fast path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputTensorContract {
    pub shape: [i32; 4],
    pub dim_size: usize,
    pub format: i32,
    pub count: usize,
    pub mem_size: usize,
    pub physical_address: u64,
    pub mem_type: i32,
    pub qscale: f32,
    pub zero_point: i32,
    pub pixel_format: i32,
    pub aligned: bool,
    pub mean: [f32; 3],
    pub scale: [f32; 3],
}

impl InputTensorContract {
    pub fn summary(self) -> String {
        format!(
            "AKARS_TPU_INPUT shape={}x{}x{}x{} dim_size={} format={} count={} mem_size={} \
             paddr=0x{:x} mem_type={} pixel_format={} aligned={} qscale={:.6} zero_point={} \
             mean={:.3},{:.3},{:.3} scale={:.3},{:.3},{:.3}",
            self.shape[0],
            self.shape[1],
            self.shape[2],
            self.shape[3],
            self.dim_size,
            self.format,
            self.count,
            self.mem_size,
            self.physical_address,
            self.mem_type,
            self.pixel_format,
            u8::from(self.aligned),
            self.qscale,
            self.zero_point,
            self.mean[0],
            self.mean[1],
            self.mean[2],
            self.scale[0],
            self.scale[1],
            self.scale[2],
        )
    }
}

const VPSS_RGB_INPUT_SHAPE: [i32; 4] = [1, 3, 640, 640];
const CVI_FMT_UINT8_VALUE: i32 = 7;
const CVI_PIXEL_FORMAT_RGB_PLANAR_VALUE: i32 = 2;

fn logical_input_dimensions(contract: InputTensorContract) -> Result<(i32, i32), TpuError> {
    if contract.dim_size == 4
        && contract.shape[0] == 1
        && contract.shape[1] == 3
        && contract.shape[2] > 0
        && contract.shape[3] > 0
    {
        return Ok((contract.shape[3], contract.shape[2]));
    }

    if contract.aligned
        && contract.pixel_format == CVI_PIXEL_FORMAT_RGB_PLANAR_VALUE
        && contract.count % 3 == 0
        && contract.mem_size >= contract.count
    {
        // Some CVI runtime builds expose aligned RGB-planar input storage as
        // a flat tensor. Recover a square logical image without hard-coding
        // 640 so reduced-resolution aligned models share the same path.
        let pixels = contract.count / 3;
        if let Some(side) = (32usize..=2048)
            .step_by(32)
            .find(|side| side.saturating_mul(*side) == pixels)
        {
            let side = i32::try_from(side)
                .map_err(|_| TpuError::new("logical input dimension exceeds i32"))?;
            return Ok((side, side));
        }
    }

    Err(TpuError::new(format!(
        "cannot recover logical model input dimensions: {}",
        contract.summary()
    )))
}

fn validate_vpss_rgb_contract(contract: InputTensorContract) -> Result<(), TpuError> {
    let floats_match = |actual: f32, expected: f32| (actual - expected).abs() <= 1.0e-6;
    let means_match = contract.mean.iter().all(|value| floats_match(*value, 0.0));
    let scales_match = contract.scale.iter().all(|value| floats_match(*value, 1.0));
    let required_size = 640usize * 640 * 3;
    let internal_dma_last = contract
        .physical_address
        .checked_add((required_size - 1) as u64);
    if contract.dim_size != 4
        || contract.shape != VPSS_RGB_INPUT_SHAPE
        || contract.format != CVI_FMT_UINT8_VALUE
        || contract.count != required_size
        || contract.mem_size < required_size
        || contract.physical_address == 0
        || internal_dma_last.is_none_or(|last| last > u64::from(u32::MAX))
        || contract.pixel_format != CVI_PIXEL_FORMAT_RGB_PLANAR_VALUE
        || contract.aligned
        || !floats_match(contract.qscale, 1.0)
        || contract.zero_point != 0
        || !means_match
        || !scales_match
    {
        return Err(TpuError::new(format!(
            "VPSS RGB fast path does not match model input contract: {}",
            contract.summary()
        )));
    }
    Ok(())
}

/// Pixel layouts accepted by the SG2002 CVI runtime physical-frame API.
///
/// The numeric values are part of the pinned `cviruntime.h` ABI.
#[repr(i32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalPixelFormat {
    RgbPacked    = 0,
    BgrPacked    = 1,
    RgbPlanar    = 2,
    BgrPlanar    = 3,
    YuvNv12      = 11,
    YuvNv21      = 12,
    Yuv420Planar = 13,
    Grayscale    = 15,
}

/// A batch of VPSS-compatible physical frames for a model compiled with both
/// fused preprocessing and aligned input enabled.
#[derive(Clone, Copy, Debug)]
pub struct AlignedPhysicalFrames<'a> {
    /// One physical base address per batch frame, as required by
    /// `CVI_NN_SetTensorWithAlignedFrames`.
    pub frame_paddrs: &'a [u64],
    pub pixel_format: PhysicalPixelFormat,
    /// Original image size used to map output boxes back from model space.
    pub source_width: i32,
    pub source_height: i32,
}

#[derive(Debug)]
pub struct TpuError(String);

impl TpuError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for TpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for TpuError {}

#[cfg(all(target_arch = "riscv64", not(akars_no_tpu)))]
mod imp {
    use std::{
        ffi::{CString, c_char, c_int, c_void},
        os::unix::ffi::OsStrExt,
        path::Path,
        ptr, slice,
        time::Instant,
    };

    use super::{
        AlignedPhysicalFrames, CameraFrame, Detection, InferTiming, InferenceConfig,
        InputTensorContract, PlanarYuvFrame, TpuError, logical_input_dimensions,
        validate_vpss_rgb_contract,
    };
    use crate::{
        detector::{
            correct_yolo_boxes, nms, parse_yolov8_i8_output, parse_yolov8_output,
            parse_yolov8_u8_output,
        },
        image_bridge,
    };

    const CVI_FMT_FP32: i32 = 0;
    const CVI_FMT_BF16: i32 = 3;
    const CVI_FMT_INT16: i32 = 4;
    const CVI_FMT_INT8: i32 = 6;
    const CVI_FMT_UINT8: i32 = 7;
    const CVI_RC_SUCCESS: i32 = 0;
    const CVI_DIM_MAX: usize = 6;

    #[repr(C)]
    #[derive(Clone, Copy, Debug)]
    struct CviShape {
        dim: [i32; CVI_DIM_MAX],
        dim_size: usize,
    }

    #[repr(C)]
    #[derive(Debug)]
    struct CviTensor {
        name: *mut c_char,
        shape: CviShape,
        fmt: i32,
        count: usize,
        mem_size: usize,
        sys_mem: *mut u8,
        paddr: u64,
        mem_type: i32,
        qscale: f32,
        zero_point: c_int,
        pixel_format: i32,
        aligned: bool,
        mean: [f32; 3],
        scale: [f32; 3],
        owner: *mut c_void,
        reserved: [c_char; 32],
    }

    type CviModelHandle = *mut c_void;

    unsafe extern "C" {
        fn CVI_NN_RegisterModel(model_file: *const c_char, model: *mut CviModelHandle) -> i32;
        fn CVI_NN_GetInputOutputTensors(
            model: CviModelHandle,
            inputs: *mut *mut CviTensor,
            input_num: *mut i32,
            outputs: *mut *mut CviTensor,
            output_num: *mut i32,
        ) -> i32;
        fn CVI_NN_GetTensorByName(
            name: *const c_char,
            tensors: *mut CviTensor,
            num: i32,
        ) -> *mut CviTensor;
        fn CVI_NN_TensorPtr(tensor: *mut CviTensor) -> *mut c_void;
        fn CVI_NN_TensorShape(tensor: *mut CviTensor) -> CviShape;
        fn CVI_NN_SetTensorPtr(tensor: *mut CviTensor, mem: *mut c_void) -> i32;
        fn CVI_NN_SetTensorPhysicalAddr(tensor: *mut CviTensor, paddr: u64) -> i32;
        fn CVI_NN_SetTensorWithAlignedFrames(
            tensor: *mut CviTensor,
            frame_paddrs: *mut u64,
            frame_num: i32,
            pixel_format: i32,
        ) -> i32;
        fn CVI_NN_Forward(
            model: CviModelHandle,
            inputs: *mut CviTensor,
            input_num: i32,
            outputs: *mut CviTensor,
            output_num: i32,
        ) -> i32;
        fn CVI_NN_CleanupModel(model: CviModelHandle) -> i32;
    }

    pub struct YoloModel {
        model: CviModelHandle,
        inputs: *mut CviTensor,
        input_num: i32,
        outputs: *mut CviTensor,
        output_num: i32,
        input: *mut CviTensor,
        input_sys_mem: *mut c_void,
        input_h: i32,
        input_w: i32,
        output_shapes: Vec<CviShape>,
        preprocessor: image_bridge::ImagePreprocessor,
    }

    impl YoloModel {
        pub fn open(path: &Path) -> Result<Self, TpuError> {
            let c_path = CString::new(path.as_os_str().as_bytes())
                .map_err(|_| TpuError::new("model path contains NUL byte"))?;
            let mut model: CviModelHandle = ptr::null_mut();
            let rc = unsafe { CVI_NN_RegisterModel(c_path.as_ptr(), &mut model) };
            if rc != CVI_RC_SUCCESS {
                return Err(TpuError::new(format!("CVI_NN_RegisterModel failed: {rc}")));
            }

            let mut inputs = ptr::null_mut();
            let mut outputs = ptr::null_mut();
            let mut input_num = 0;
            let mut output_num = 0;
            let rc = unsafe {
                CVI_NN_GetInputOutputTensors(
                    model,
                    &mut inputs,
                    &mut input_num,
                    &mut outputs,
                    &mut output_num,
                )
            };
            if rc != CVI_RC_SUCCESS {
                unsafe {
                    CVI_NN_CleanupModel(model);
                }
                return Err(TpuError::new(format!(
                    "CVI_NN_GetInputOutputTensors failed: {rc}"
                )));
            }

            let input = unsafe { CVI_NN_GetTensorByName(ptr::null(), inputs, input_num) };
            if input.is_null() {
                unsafe {
                    CVI_NN_CleanupModel(model);
                }
                return Err(TpuError::new("default input tensor not found"));
            }
            let input_sys_mem = unsafe { CVI_NN_TensorPtr(input) };
            if input_sys_mem.is_null() {
                unsafe {
                    CVI_NN_CleanupModel(model);
                }
                return Err(TpuError::new("default input tensor buffer is null"));
            }

            let input_tensor = unsafe { &*input };
            let input_contract = InputTensorContract {
                shape: [
                    input_tensor.shape.dim[0],
                    input_tensor.shape.dim[1],
                    input_tensor.shape.dim[2],
                    input_tensor.shape.dim[3],
                ],
                dim_size: input_tensor.shape.dim_size,
                format: input_tensor.fmt,
                count: input_tensor.count,
                mem_size: input_tensor.mem_size,
                physical_address: input_tensor.paddr,
                mem_type: input_tensor.mem_type,
                qscale: input_tensor.qscale,
                zero_point: input_tensor.zero_point,
                pixel_format: input_tensor.pixel_format,
                aligned: input_tensor.aligned,
                mean: input_tensor.mean,
                scale: input_tensor.scale,
            };
            let (input_w, input_h) = match logical_input_dimensions(input_contract) {
                Ok(dimensions) => dimensions,
                Err(error) => {
                    unsafe {
                        CVI_NN_CleanupModel(model);
                    }
                    return Err(error);
                }
            };
            let outputs_slice = unsafe { slice::from_raw_parts_mut(outputs, output_num as usize) };
            let output_shapes = outputs_slice
                .iter_mut()
                .map(|tensor| unsafe { CVI_NN_TensorShape(tensor as *mut CviTensor) })
                .collect();

            Ok(Self {
                model,
                inputs,
                input_num,
                outputs,
                output_num,
                input,
                input_sys_mem,
                input_h,
                input_w,
                output_shapes,
                preprocessor: image_bridge::ImagePreprocessor::new(),
            })
        }

        pub fn input_contract(&self) -> InputTensorContract {
            let tensor = unsafe { &*self.input };
            InputTensorContract {
                shape: [
                    tensor.shape.dim[0],
                    tensor.shape.dim[1],
                    tensor.shape.dim[2],
                    tensor.shape.dim[3],
                ],
                dim_size: tensor.shape.dim_size,
                format: tensor.fmt,
                count: tensor.count,
                mem_size: tensor.mem_size,
                physical_address: tensor.paddr,
                mem_type: tensor.mem_type,
                qscale: tensor.qscale,
                zero_point: tensor.zero_point,
                pixel_format: tensor.pixel_format,
                aligned: tensor.aligned,
                mean: tensor.mean,
                scale: tensor.scale,
            }
        }

        pub const fn input_dimensions(&self) -> (i32, i32) {
            (self.input_w, self.input_h)
        }

        pub fn infer(
            &mut self,
            frame: &CameraFrame,
            config: InferenceConfig,
        ) -> Result<Vec<Detection>, TpuError> {
            self.infer_timed(frame, config, None)
        }

        pub fn infer_timed(
            &mut self,
            frame: &CameraFrame,
            config: InferenceConfig,
            timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            // A previous physical-frame inference may have rebound this tensor.
            // Explicitly restore the runtime-owned system buffer before CPU
            // preprocessing so callers can safely alternate input modes.
            let rc = unsafe { CVI_NN_SetTensorPtr(self.input, self.input_sys_mem) };
            if rc != CVI_RC_SUCCESS {
                return Err(TpuError::new(format!(
                    "CVI_NN_SetTensorPtr failed while restoring system input: {rc}"
                )));
            }
            let input_ptr = self.input_sys_mem as *mut u8;

            let input_len = rgb_tensor_len(self.input_w, self.input_h)?;
            let input_tensor = unsafe { &*self.input };
            if input_tensor.mem_size < input_len {
                return Err(TpuError::new(format!(
                    "input tensor buffer is too small: mem_size={} required={input_len}",
                    input_tensor.mem_size
                )));
            }
            let input = unsafe { slice::from_raw_parts_mut(input_ptr, input_len) };

            let pre_start = Instant::now();
            let preprocess = self
                .preprocessor
                .mjpeg_to_rgb_planar(&frame.jpeg, input, self.input_w, self.input_h)
                .map_err(|err| TpuError::new(format!("MJPEG decode/preprocess failed: {err}")))?;
            let preprocess_us = pre_start.elapsed().as_micros() as i64;

            let image_w = if preprocess.src_w > 0 {
                preprocess.src_w
            } else {
                frame.width as i32
            };
            let image_h = if preprocess.src_h > 0 {
                preprocess.src_h
            } else {
                frame.height as i32
            };
            self.forward_and_postprocess(config, image_w, image_h, preprocess_us, timing)
        }

        pub fn infer_yuv_timed(
            &mut self,
            frame: &PlanarYuvFrame,
            config: InferenceConfig,
            timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            let rc = unsafe { CVI_NN_SetTensorPtr(self.input, self.input_sys_mem) };
            if rc != CVI_RC_SUCCESS {
                return Err(TpuError::new(format!(
                    "CVI_NN_SetTensorPtr failed while restoring system input: {rc}"
                )));
            }
            let input_len = rgb_tensor_len(self.input_w, self.input_h)?;
            let input_tensor = unsafe { &*self.input };
            if input_tensor.mem_size < input_len {
                return Err(TpuError::new(format!(
                    "input tensor buffer is too small: mem_size={} required={input_len}",
                    input_tensor.mem_size
                )));
            }
            let input =
                unsafe { slice::from_raw_parts_mut(self.input_sys_mem as *mut u8, input_len) };

            let pre_start = Instant::now();
            self.preprocessor
                .yuv_to_rgb_planar(
                    &frame.data,
                    frame.width,
                    frame.height,
                    frame.format,
                    input,
                    self.input_w,
                    self.input_h,
                )
                .map_err(|err| TpuError::new(format!("JPU YUV preprocess failed: {err}")))?;
            let preprocess_us = pre_start.elapsed().as_micros() as i64;
            let image_w = i32::try_from(frame.width)
                .map_err(|_| TpuError::new("source image width exceeds i32"))?;
            let image_h = i32::try_from(frame.height)
                .map_err(|_| TpuError::new("source image height exceeds i32"))?;
            self.forward_and_postprocess(config, image_w, image_h, preprocess_us, timing)
        }

        /// Copy a VPSS-produced compact RGB-planar tensor into the ordinary
        /// (non-aligned) model input, then run TPU inference.
        pub fn infer_rgb_planar_timed(
            &mut self,
            rgb: &[u8],
            source_width: i32,
            source_height: i32,
            config: InferenceConfig,
            timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            let rc = unsafe { CVI_NN_SetTensorPtr(self.input, self.input_sys_mem) };
            if rc != CVI_RC_SUCCESS {
                return Err(TpuError::new(format!(
                    "CVI_NN_SetTensorPtr failed while restoring system input: {rc}"
                )));
            }
            let input_len = rgb_tensor_len(self.input_w, self.input_h)?;
            if rgb.len() != input_len {
                return Err(TpuError::new(format!(
                    "VPSS RGB tensor size mismatch: got={} required={input_len}",
                    rgb.len()
                )));
            }
            let input_tensor = unsafe { &*self.input };
            if input_tensor.mem_size < input_len {
                return Err(TpuError::new(format!(
                    "input tensor buffer is too small: mem_size={} required={input_len}",
                    input_tensor.mem_size
                )));
            }
            let preprocess_start = Instant::now();
            let input =
                unsafe { slice::from_raw_parts_mut(self.input_sys_mem as *mut u8, input_len) };
            input.copy_from_slice(rgb);
            let preprocess_us = preprocess_start.elapsed().as_micros() as i64;
            self.forward_and_postprocess(config, source_width, source_height, preprocess_us, timing)
        }

        /// Bind already-preprocessed contiguous device memory as the input
        /// tensor and run inference without a CPU copy.
        ///
        /// # Safety
        ///
        /// `paddr` must identify a live, DMA-visible buffer that is large
        /// enough for the model input and remains owned by the caller until
        /// this blocking call returns. The buffer must already match the
        /// input tensor's shape, format, quantization and alignment.
        pub unsafe fn infer_physical_tensor_timed(
            &mut self,
            paddr: u64,
            source_width: i32,
            source_height: i32,
            config: InferenceConfig,
            timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            validate_physical_input(paddr, source_width, source_height)?;
            let rc = unsafe { CVI_NN_SetTensorPhysicalAddr(self.input, paddr) };
            if rc != CVI_RC_SUCCESS {
                return Err(TpuError::new(format!(
                    "CVI_NN_SetTensorPhysicalAddr failed: {rc}"
                )));
            }
            self.forward_and_postprocess(config, source_width, source_height, 0, timing)
        }

        /// Import one VPSS RGB-planar frame through the runtime's explicit
        /// frame API and run inference.
        ///
        /// For an ordinary `aligned=false` model, the pinned CVI runtime uses
        /// TDMA to compact the VPSS frame into the model-owned input tensor.
        /// Unlike the implicit copy reached through
        /// `CVI_NN_SetTensorPhysicalAddr`, this API returns the TDMA result.
        /// With a 640-byte row stride, the SG2002 VPSS output already matches
        /// the runtime's 64-byte source alignment.
        ///
        /// # Safety
        ///
        /// `paddr` must identify a live, DMA-visible RGB-planar frame matching
        /// the model input dimensions, and remain valid until this blocking
        /// call returns.
        pub unsafe fn infer_vpss_rgb_timed(
            &mut self,
            paddr: u64,
            source_width: i32,
            source_height: i32,
            config: InferenceConfig,
            timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            let preprocess_us = unsafe { self.preload_vpss_rgb(paddr) }?;
            self.forward_and_postprocess(config, source_width, source_height, preprocess_us, timing)
        }

        /// Run the normal physical-frame path and, before forward, compare the
        /// VPSS mapping byte-for-byte with the runtime-owned tensor mapping.
        /// This is a diagnostic path and intentionally scans the full tensor.
        ///
        /// # Safety
        ///
        /// The same requirements as [`Self::infer_vpss_rgb_timed`] apply.
        pub unsafe fn infer_vpss_rgb_verified_timed(
            &mut self,
            paddr: u64,
            expected_rgb: &[u8],
            source_width: i32,
            source_height: i32,
            config: InferenceConfig,
            timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            let preprocess_us = unsafe { self.preload_vpss_rgb(paddr) }?;
            self.verify_preloaded_vpss_input(expected_rgb)?;
            self.forward_and_postprocess(config, source_width, source_height, preprocess_us, timing)
        }

        unsafe fn preload_vpss_rgb(&mut self, paddr: u64) -> Result<i64, TpuError> {
            validate_physical_input(paddr, 1, 1)?;
            let contract = self.input_contract();
            validate_vpss_rgb_contract(contract)?;
            let input_len = rgb_tensor_len(self.input_w, self.input_h)?;
            validate_tpu_dma_range(paddr, input_len)?;
            let preprocess_start = Instant::now();
            let mut frame_paddrs = [paddr];
            let rc = unsafe {
                CVI_NN_SetTensorWithAlignedFrames(
                    self.input,
                    frame_paddrs.as_mut_ptr(),
                    1,
                    super::PhysicalPixelFormat::RgbPlanar as i32,
                )
            };
            let preprocess_us = preprocess_start.elapsed().as_micros() as i64;
            if rc != CVI_RC_SUCCESS {
                return Err(TpuError::new(format!(
                    "CVI_NN_SetTensorWithAlignedFrames failed: {rc}"
                )));
            }
            Ok(preprocess_us)
        }

        fn verify_preloaded_vpss_input(&self, expected_rgb: &[u8]) -> Result<(), TpuError> {
            let input_len = rgb_tensor_len(self.input_w, self.input_h)?;
            if expected_rgb.len() < input_len {
                return Err(TpuError::new(format!(
                    "VPSS verification mapping is too small: got={} required={input_len}",
                    expected_rgb.len()
                )));
            }
            let actual = unsafe {
                slice::from_raw_parts(self.input_sys_mem.cast::<u8>() as *const u8, input_len)
            };
            let expected = &expected_rgb[..input_len];
            let mut mismatch_count = 0usize;
            let mut first_mismatch = None;
            for (index, (&source, &tensor)) in expected.iter().zip(actual).enumerate() {
                if source != tensor {
                    mismatch_count += 1;
                    if first_mismatch.is_none() {
                        first_mismatch = Some((index, source, tensor));
                    }
                }
            }
            let (first_index, source_byte, tensor_byte) = first_mismatch
                .map(|(index, source, tensor)| (index as i64, source, tensor))
                .unwrap_or((-1, 0, 0));
            println!(
                "AKARS_TPU_INPUT_VERIFY bytes={} source_hash={:016x} tensor_hash={:016x} \
                 mismatches={} first_index={} source_byte={} tensor_byte={}",
                input_len,
                fnv1a64(expected),
                fnv1a64(actual),
                mismatch_count,
                first_index,
                source_byte,
                tensor_byte,
            );
            Ok(())
        }

        /// Compare the current runtime-owned input (normally produced by the
        /// known-good CPU YUV preprocessor) with one VPSS RGB-planar frame.
        /// This diagnostic distinguishes channel ordering, CSC errors and
        /// broken letterbox generation without modifying either buffer.
        pub fn compare_current_input_with_vpss(&self, vpss: &[u8]) -> Result<(), TpuError> {
            let input_len = rgb_tensor_len(self.input_w, self.input_h)?;
            if vpss.len() != input_len {
                return Err(TpuError::new(format!(
                    "VPSS comparison size mismatch: got={} required={input_len}",
                    vpss.len()
                )));
            }
            let cpu = unsafe {
                slice::from_raw_parts(self.input_sys_mem.cast::<u8>() as *const u8, input_len)
            };
            let width = usize::try_from(self.input_w)
                .map_err(|_| TpuError::new("negative model input width"))?;
            let height = usize::try_from(self.input_h)
                .map_err(|_| TpuError::new("negative model input height"))?;
            let plane_size = width
                .checked_mul(height)
                .ok_or_else(|| TpuError::new("model input plane size overflow"))?;
            let mut abs_sum = [0_u64; 3];
            let mut max_error = [0_u8; 3];
            let mut exact = [0_usize; 3];
            for channel in 0..3 {
                let start = channel * plane_size;
                for (&hardware, &software) in vpss[start..start + plane_size]
                    .iter()
                    .zip(&cpu[start..start + plane_size])
                {
                    let error = hardware.abs_diff(software);
                    abs_sum[channel] += u64::from(error);
                    max_error[channel] = max_error[channel].max(error);
                    exact[channel] += usize::from(error == 0);
                }
            }

            let mut rb_swap_abs_sum = 0_u64;
            for index in 0..plane_size {
                rb_swap_abs_sum += u64::from(vpss[index].abs_diff(cpu[2 * plane_size + index]));
                rb_swap_abs_sum +=
                    u64::from(vpss[plane_size + index].abs_diff(cpu[plane_size + index]));
                rb_swap_abs_sum += u64::from(vpss[2 * plane_size + index].abs_diff(cpu[index]));
            }

            let content_top = (height.saturating_sub(480)) / 2;
            let content_bottom = (content_top + 480).min(height);
            let mut padding_nonzero = [0_usize; 3];
            let mut hardware_min = [u8::MAX; 3];
            let mut hardware_max = [u8::MIN; 3];
            let mut hardware_sum = [0_u64; 3];
            let mut software_min = [u8::MAX; 3];
            let mut software_max = [u8::MIN; 3];
            let mut software_sum = [0_u64; 3];
            for channel in 0..3 {
                let start = channel * plane_size;
                for row in 0..height {
                    let row_start = start + row * width;
                    let row_end = row_start + width;
                    if row < content_top || row >= content_bottom {
                        padding_nonzero[channel] += vpss[row_start..row_end]
                            .iter()
                            .filter(|value| **value != 0)
                            .count();
                        continue;
                    }
                    for (&hardware, &software) in vpss[row_start..row_end]
                        .iter()
                        .zip(&cpu[row_start..row_end])
                    {
                        hardware_min[channel] = hardware_min[channel].min(hardware);
                        hardware_max[channel] = hardware_max[channel].max(hardware);
                        hardware_sum[channel] += u64::from(hardware);
                        software_min[channel] = software_min[channel].min(software);
                        software_max[channel] = software_max[channel].max(software);
                        software_sum[channel] += u64::from(software);
                    }
                }
            }
            let content_pixels = width * content_bottom.saturating_sub(content_top);
            let mean = |sum: u64| {
                if content_pixels == 0 {
                    0
                } else {
                    sum.saturating_mul(1_000) / content_pixels as u64
                }
            };
            let mae = |sum: u64, count: usize| {
                if count == 0 {
                    0
                } else {
                    sum.saturating_mul(1_000) / count as u64
                }
            };
            println!(
                "AKARS_VPSS_CPU_COMPARE bytes={} vpss_hash={:016x} cpu_hash={:016x} \
                 mae_x1000={},{},{} max_error={},{},{} exact={},{},{} rb_swap_mae_x1000={} \
                 padding_nonzero={},{},{} vpss_minmax={}-{},{}-{},{}-{} vpss_mean_x1000={},{},{} \
                 cpu_minmax={}-{},{}-{},{}-{} cpu_mean_x1000={},{},{}",
                input_len,
                fnv1a64(vpss),
                fnv1a64(cpu),
                mae(abs_sum[0], plane_size),
                mae(abs_sum[1], plane_size),
                mae(abs_sum[2], plane_size),
                max_error[0],
                max_error[1],
                max_error[2],
                exact[0],
                exact[1],
                exact[2],
                mae(rb_swap_abs_sum, input_len),
                padding_nonzero[0],
                padding_nonzero[1],
                padding_nonzero[2],
                hardware_min[0],
                hardware_max[0],
                hardware_min[1],
                hardware_max[1],
                hardware_min[2],
                hardware_max[2],
                mean(hardware_sum[0]),
                mean(hardware_sum[1]),
                mean(hardware_sum[2]),
                software_min[0],
                software_max[0],
                software_min[1],
                software_max[1],
                software_min[2],
                software_max[2],
                mean(software_sum[0]),
                mean(software_sum[1]),
                mean(software_sum[2]),
            );
            Ok(())
        }

        /// Bind VPSS-produced aligned frames to a fused-preprocess model.
        ///
        /// This refuses ordinary models: `input.aligned` must be set by a
        /// model compiled with `--fuse_preprocess --aligned_input` and its
        /// declared pixel format must match the supplied VPSS frame format.
        ///
        /// # Safety
        ///
        /// Every address must identify a live, DMA-visible VPSS frame with the
        /// alignment and layout declared by `pixel_format`. The buffers must
        /// remain owned by the caller until this blocking call returns.
        pub unsafe fn infer_aligned_physical_timed(
            &mut self,
            frames: AlignedPhysicalFrames<'_>,
            config: InferenceConfig,
            timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            if frames.frame_paddrs.is_empty() || frames.frame_paddrs.contains(&0) {
                return Err(TpuError::new(
                    "aligned physical frame addresses must be non-empty and non-zero",
                ));
            }
            let frame_num = i32::try_from(frames.frame_paddrs.len())
                .map_err(|_| TpuError::new("too many physical frames"))?;
            validate_physical_input(
                frames.frame_paddrs[0],
                frames.source_width,
                frames.source_height,
            )?;

            let input = unsafe { &*self.input };
            if !input.aligned {
                return Err(TpuError::new(
                    "model input is not aligned; rebuild with --fuse_preprocess --aligned_input",
                ));
            }
            if input.pixel_format != frames.pixel_format as i32 {
                return Err(TpuError::new(format!(
                    "physical frame format mismatch: model={} frame={}",
                    input.pixel_format, frames.pixel_format as i32
                )));
            }

            let rc = unsafe {
                CVI_NN_SetTensorWithAlignedFrames(
                    self.input,
                    frames.frame_paddrs.as_ptr().cast_mut(),
                    frame_num,
                    frames.pixel_format as i32,
                )
            };
            if rc != CVI_RC_SUCCESS {
                return Err(TpuError::new(format!(
                    "CVI_NN_SetTensorWithAlignedFrames failed: {rc}"
                )));
            }
            self.forward_and_postprocess(
                config,
                frames.source_width,
                frames.source_height,
                0,
                timing,
            )
        }

        /// Run inference on a standalone image and write a copy with the
        /// detection boxes drawn to out_path.
        pub fn detect_image(
            &mut self,
            image: &[u8],
            out_path: &Path,
            config: InferenceConfig,
        ) -> Result<Vec<Detection>, TpuError> {
            let frame = CameraFrame {
                jpeg: image.to_vec(),
                width: 0,
                height: 0,
            };
            let detections = self.infer(&frame, config)?;

            image_bridge::draw_detections(image, &detections, out_path)
                .map_err(|err| TpuError::new(format!("failed to write annotated image: {err}")))?;
            Ok(detections)
        }

        fn get_detections(&mut self, config: InferenceConfig) -> Result<Vec<Detection>, TpuError> {
            if self.output_num < 1 || self.output_shapes.is_empty() {
                return Err(TpuError::new("model has no output tensor"));
            }
            let output = unsafe { &mut *self.outputs };
            let shape = self.output_shapes[0];
            let count = output.count;
            let ptr = unsafe { CVI_NN_TensorPtr(output as *mut CviTensor) };
            if ptr.is_null() {
                return Err(TpuError::new("output tensor pointer is null"));
            }

            let output_shape = [shape.dim[0], shape.dim[1], shape.dim[2], shape.dim[3]];
            match output.fmt {
                CVI_FMT_FP32 => {
                    let data = unsafe { slice::from_raw_parts(ptr as *const f32, count) };
                    Ok(parse_yolov8_output(
                        data,
                        output_shape,
                        config.classes_num,
                        config.confidence_threshold,
                    ))
                }
                CVI_FMT_INT8 => {
                    let data = unsafe { slice::from_raw_parts(ptr as *const i8, count) };
                    Ok(parse_yolov8_i8_output(
                        data,
                        output_shape,
                        config.classes_num,
                        config.confidence_threshold,
                        output.qscale,
                        output.zero_point,
                    ))
                }
                CVI_FMT_UINT8 => {
                    let data = unsafe { slice::from_raw_parts(ptr as *const u8, count) };
                    Ok(parse_yolov8_u8_output(
                        data,
                        output_shape,
                        config.classes_num,
                        config.confidence_threshold,
                        output.qscale,
                        output.zero_point,
                    ))
                }
                CVI_FMT_BF16 | CVI_FMT_INT16 => {
                    let data = tensor_to_f32(output, ptr, count)?;
                    Ok(parse_yolov8_output(
                        &data,
                        output_shape,
                        config.classes_num,
                        config.confidence_threshold,
                    ))
                }
                other => Err(TpuError::new(format!(
                    "unsupported output tensor format: {other}"
                ))),
            }
        }

        fn forward_and_postprocess(
            &mut self,
            config: InferenceConfig,
            image_w: i32,
            image_h: i32,
            preprocess_us: i64,
            timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            let fwd_start = Instant::now();
            let rc = unsafe {
                CVI_NN_Forward(
                    self.model,
                    self.inputs,
                    self.input_num,
                    self.outputs,
                    self.output_num,
                )
            };
            let forward_us = fwd_start.elapsed().as_micros() as i64;
            if rc != CVI_RC_SUCCESS {
                return Err(TpuError::new(format!("CVI_NN_Forward failed: {rc}")));
            }

            let post_start = Instant::now();
            let mut detections = self.get_detections(config)?;
            nms(&mut detections, config.iou_threshold);
            correct_yolo_boxes(
                &mut detections,
                image_h,
                image_w,
                self.input_h,
                self.input_w,
            );
            let postprocess_us = post_start.elapsed().as_micros() as i64;

            if let Some(t) = timing {
                *t = InferTiming {
                    preprocess_us,
                    forward_us,
                    postprocess_us,
                };
            }
            Ok(detections)
        }
    }

    impl Drop for YoloModel {
        fn drop(&mut self) {
            if !self.model.is_null() {
                unsafe {
                    CVI_NN_CleanupModel(self.model);
                }
            }
        }
    }

    fn rgb_tensor_len(width: i32, height: i32) -> Result<usize, TpuError> {
        if width <= 0 || height <= 0 {
            return Err(TpuError::new("input tensor dimensions must be positive"));
        }
        (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(3))
            .ok_or_else(|| TpuError::new("input tensor dimensions overflow"))
    }

    fn validate_physical_input(
        paddr: u64,
        source_width: i32,
        source_height: i32,
    ) -> Result<(), TpuError> {
        if paddr == 0 {
            return Err(TpuError::new("physical tensor address must be non-zero"));
        }
        if source_width <= 0 || source_height <= 0 {
            return Err(TpuError::new(
                "physical input source dimensions must be positive",
            ));
        }
        Ok(())
    }

    fn validate_tpu_dma_range(paddr: u64, size: usize) -> Result<(), TpuError> {
        let last = size
            .checked_sub(1)
            .and_then(|offset| paddr.checked_add(offset as u64))
            .ok_or_else(|| TpuError::new("physical tensor DMA range overflow"))?;
        if last > u64::from(u32::MAX) {
            return Err(TpuError::new(format!(
                "physical tensor DMA range exceeds SG2002 32-bit address space: paddr=0x{paddr:x} \
                 size={size}"
            )));
        }
        Ok(())
    }

    fn fnv1a64(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    fn tensor_to_f32(
        tensor: &CviTensor,
        ptr: *mut c_void,
        count: usize,
    ) -> Result<Vec<f32>, TpuError> {
        match tensor.fmt {
            CVI_FMT_BF16 => {
                let src = unsafe { slice::from_raw_parts(ptr as *const u16, count) };
                Ok(src
                    .iter()
                    .map(|v| f32::from_bits((*v as u32) << 16))
                    .collect())
            }
            CVI_FMT_INT16 => {
                let src = unsafe { slice::from_raw_parts(ptr as *const i16, count) };
                Ok(src
                    .iter()
                    .map(|v| (i32::from(*v) - tensor.zero_point) as f32 * tensor.qscale)
                    .collect())
            }
            other => Err(TpuError::new(format!(
                "unsupported output tensor format: {other}"
            ))),
        }
    }
}

#[cfg(any(not(target_arch = "riscv64"), akars_no_tpu))]
mod imp {
    use std::path::Path;

    use super::{
        AlignedPhysicalFrames, CameraFrame, Detection, InferTiming, InferenceConfig,
        InputTensorContract, PlanarYuvFrame, TpuError,
    };

    pub struct YoloModel;

    impl YoloModel {
        pub fn open(_path: &Path) -> Result<Self, TpuError> {
            Err(TpuError::new(
                "akars was built without SG2002 TPU runtime support",
            ))
        }

        pub fn input_contract(&self) -> InputTensorContract {
            unreachable!("host TPU stub cannot own a model")
        }

        pub fn input_dimensions(&self) -> (i32, i32) {
            unreachable!("host TPU stub cannot own a model")
        }

        pub fn infer(
            &mut self,
            _frame: &CameraFrame,
            _config: InferenceConfig,
        ) -> Result<Vec<Detection>, TpuError> {
            Err(TpuError::new(
                "akars was built without SG2002 TPU runtime support",
            ))
        }

        pub fn infer_timed(
            &mut self,
            _frame: &CameraFrame,
            _config: InferenceConfig,
            _timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            Err(TpuError::new(
                "akars was built without SG2002 TPU runtime support",
            ))
        }

        pub fn infer_yuv_timed(
            &mut self,
            _frame: &PlanarYuvFrame,
            _config: InferenceConfig,
            _timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            Err(TpuError::new(
                "akars was built without SG2002 TPU runtime support",
            ))
        }

        pub fn infer_rgb_planar_timed(
            &mut self,
            _rgb: &[u8],
            _source_width: i32,
            _source_height: i32,
            _config: InferenceConfig,
            _timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            Err(TpuError::new(
                "RGB planar inference requires SG2002 TPU runtime support",
            ))
        }

        /// # Safety
        ///
        /// See the SG2002 implementation. This host stub never dereferences
        /// the supplied physical address.
        pub unsafe fn infer_physical_tensor_timed(
            &mut self,
            _paddr: u64,
            _source_width: i32,
            _source_height: i32,
            _config: InferenceConfig,
            _timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            Err(TpuError::new(
                "physical tensor inference requires SG2002 TPU runtime support",
            ))
        }

        /// # Safety
        ///
        /// See the SG2002 implementation. This host stub never dereferences
        /// the supplied physical address.
        pub unsafe fn infer_vpss_rgb_timed(
            &mut self,
            _paddr: u64,
            _source_width: i32,
            _source_height: i32,
            _config: InferenceConfig,
            _timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            Err(TpuError::new(
                "VPSS physical inference requires SG2002 TPU runtime support",
            ))
        }

        /// # Safety
        ///
        /// See the SG2002 implementation. This host stub never dereferences
        /// the supplied physical address.
        pub unsafe fn infer_vpss_rgb_verified_timed(
            &mut self,
            _paddr: u64,
            _expected_rgb: &[u8],
            _source_width: i32,
            _source_height: i32,
            _config: InferenceConfig,
            _timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            Err(TpuError::new(
                "VPSS physical verification requires SG2002 TPU runtime support",
            ))
        }

        pub fn compare_current_input_with_vpss(&self, _vpss: &[u8]) -> Result<(), TpuError> {
            Err(TpuError::new(
                "VPSS input comparison requires SG2002 TPU runtime support",
            ))
        }

        /// # Safety
        ///
        /// See the SG2002 implementation. This host stub never dereferences
        /// the supplied physical addresses.
        pub unsafe fn infer_aligned_physical_timed(
            &mut self,
            _frames: AlignedPhysicalFrames<'_>,
            _config: InferenceConfig,
            _timing: Option<&mut InferTiming>,
        ) -> Result<Vec<Detection>, TpuError> {
            Err(TpuError::new(
                "aligned physical inference requires SG2002 TPU runtime support",
            ))
        }

        pub fn detect_image(
            &mut self,
            _image: &[u8],
            _out_path: &Path,
            _config: InferenceConfig,
        ) -> Result<Vec<Detection>, TpuError> {
            Err(TpuError::new(
                "akars was built without SG2002 TPU runtime support",
            ))
        }
    }
}

pub use imp::YoloModel;

pub fn open_model(path: impl AsRef<Path>) -> Result<YoloModel, TpuError> {
    YoloModel::open(path.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected_contract() -> InputTensorContract {
        InputTensorContract {
            shape: [1, 3, 640, 640],
            dim_size: 4,
            format: 7,
            count: 3 * 640 * 640,
            mem_size: 3 * 640 * 640,
            physical_address: 0x1000_0000,
            mem_type: 0,
            qscale: 1.0,
            zero_point: 0,
            pixel_format: PhysicalPixelFormat::RgbPlanar as i32,
            aligned: false,
            mean: [0.0; 3],
            scale: [1.0; 3],
        }
    }

    #[test]
    fn accepts_exact_vpss_rgb_model_contract() {
        assert!(validate_vpss_rgb_contract(expected_contract()).is_ok());
    }

    #[test]
    fn rejects_bgr_model_contract() {
        let mut contract = expected_contract();
        contract.pixel_format = PhysicalPixelFormat::BgrPlanar as i32;
        assert!(validate_vpss_rgb_contract(contract).is_err());
    }

    #[test]
    fn rejects_non_uint8_or_wrong_shape_contract() {
        let mut contract = expected_contract();
        contract.format = 6;
        assert!(validate_vpss_rgb_contract(contract).is_err());

        let mut contract = expected_contract();
        contract.shape = [1, 3, 320, 320];
        assert!(validate_vpss_rgb_contract(contract).is_err());
    }

    #[test]
    fn recovers_flat_aligned_rgb_planar_dimensions() {
        let mut contract = expected_contract();
        contract.shape = [1, 1, 1, 3 * 640 * 640];
        contract.aligned = true;
        assert_eq!(logical_input_dimensions(contract).unwrap(), (640, 640));

        contract.shape = [1, 1, 1, 3 * 384 * 384];
        contract.count = 3 * 384 * 384;
        contract.mem_size = contract.count;
        assert_eq!(logical_input_dimensions(contract).unwrap(), (384, 384));
    }

    #[test]
    fn rejects_unknown_flat_aligned_dimensions() {
        let mut contract = expected_contract();
        contract.shape = [1, 1, 1, 3 * 320 * 352];
        contract.count = 3 * 320 * 352;
        contract.mem_size = contract.count;
        contract.aligned = true;
        assert!(logical_input_dimensions(contract).is_err());
    }
}
