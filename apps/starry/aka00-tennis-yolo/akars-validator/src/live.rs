#![allow(dead_code)]

mod camera;
mod detector;
mod image_bridge;
mod live_camera;
mod tpu;
mod vpss_pipeline;

use std::{
    env,
    error::Error,
    path::PathBuf,
    time::{Duration, Instant},
};

use camera::{CameraFrame, PlanarYuvFrame};
use live_camera::{CameraCaptureStats, LiveCamera};
use tpu::{AlignedPhysicalFrames, InferTiming, InferenceConfig, PhysicalPixelFormat, open_model};
use vpss_pipeline::VpssRgbPipeline;

const DEFAULT_DEVICE: &str = "/dev/cvi-usb-camera0";
const DEFAULT_VPSS_DEVICE: &str = "/dev/cvi-vpss0";
const DEFAULT_FRAMES: u32 = 100;

struct Cli {
    model: PathBuf,
    device: PathBuf,
    vpss_device: PathBuf,
    frames: u32,
    duration_seconds: Option<u64>,
    report_frames: bool,
    timing_percentiles: bool,
    input: InputMode,
    verify_input: bool,
    config: InferenceConfig,
}

#[derive(Clone, Copy)]
enum InputMode {
    Mjpeg,
    JpuYuv,
    VpssRgb,
}

impl InputMode {
    fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value {
            "mjpeg" => Ok(Self::Mjpeg),
            "jpu-yuv" => Ok(Self::JpuYuv),
            "vpss-rgb" => Ok(Self::VpssRgb),
            _ => Err(format!("unsupported input mode: {value}").into()),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Mjpeg => "mjpeg_cpu",
            Self::JpuYuv => "jpu_yuv",
            Self::VpssRgb => "jpu_vpss_rgb",
        }
    }
}

#[derive(Default)]
struct TimingTotals {
    request_us: u64,
    camera_ion_us: u64,
    capture_us: u64,
    preprocess_us: u64,
    forward_us: u64,
    postprocess_us: u64,
    total_us: u64,
    max_total_us: u64,
    vpss_wall_us: u64,
    vpss_hardware_us: u64,
    vpss_hardware_max_us: u64,
}

#[derive(Clone, Copy)]
struct FrameTiming {
    request_us: u64,
    camera_ion_us: u64,
    capture_us: u64,
    preprocess_us: u64,
    forward_us: u64,
    postprocess_us: u64,
    vpss_wall_us: u64,
    vpss_hardware_us: u64,
    total_us: u64,
    top_score_q10000: u32,
}

fn main() {
    if let Err(error) = run() {
        eprintln!(
            "AKARS_LIVE_FAIL reason={}",
            error.to_string().replace(' ', "_")
        );
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let cli = parse_cli(env::args().skip(1))?;
    let mut model = open_model(&cli.model)?;
    let input_contract = model.input_contract();
    println!("{}", input_contract.summary());
    let aligned_input = input_contract.aligned;
    let (model_input_width, model_input_height) = model.input_dimensions();
    if cli.verify_input && (model_input_width != 640 || model_input_height != 640) {
        return Err("--verify-input currently requires a 640x640 model".into());
    }
    let mut camera = if matches!(cli.input, InputMode::VpssRgb) {
        None
    } else {
        Some(LiveCamera::open(&cli.device)?)
    };
    let mut vpss = if matches!(cli.input, InputMode::VpssRgb) {
        Some(VpssRgbPipeline::open(
            &cli.device,
            &cli.vpss_device,
            u32::try_from(model_input_width)?,
            u32::try_from(model_input_height)?,
        )?)
    } else {
        None
    };
    let mut mjpeg_frame = CameraFrame::default();
    let mut yuv_frame = PlanarYuvFrame::default();
    let mut totals = TimingTotals::default();
    let mut first_sequence = 0u64;
    let mut last_sequence = 0u64;
    let mut skipped_sequences = 0u64;
    let mut frames_with_detections = 0u64;
    let mut detections_total = 0u64;
    let wall_start = Instant::now();
    let duration_limit = cli.duration_seconds.map(Duration::from_secs);
    let mut frames_processed = 0u32;
    let mut timing_samples = cli.timing_percentiles.then(Vec::new);

    loop {
        if frames_processed > 0 {
            if let Some(limit) = duration_limit {
                if wall_start.elapsed() >= limit {
                    break;
                }
            } else if frames_processed >= cli.frames {
                break;
            }
        }
        let index = frames_processed;
        let total_start = Instant::now();
        let mut timing = InferTiming::default();
        let (meta, detections, request_us, camera_ion_us, vpss_wall_us, vpss_hardware_us) =
            match cli.input {
                InputMode::Mjpeg => {
                    let request_start = Instant::now();
                    let meta = camera
                        .as_mut()
                        .expect("camera exists for MJPEG")
                        .next_mjpeg(&mut mjpeg_frame, 2_000)?;
                    let request_us = request_start.elapsed().as_micros() as u64;
                    let detections =
                        model.infer_timed(&mjpeg_frame, cli.config, Some(&mut timing))?;
                    (meta, detections, request_us, 0, 0, 0)
                }
                InputMode::JpuYuv => {
                    let request_start = Instant::now();
                    let meta = camera
                        .as_mut()
                        .expect("camera exists for JPU YUV")
                        .next_yuv(&mut yuv_frame, 2_000)?;
                    let request_us = request_start.elapsed().as_micros() as u64;
                    let detections =
                        model.infer_yuv_timed(&yuv_frame, cli.config, Some(&mut timing))?;
                    (meta, detections, request_us, 0, 0, 0)
                }
                InputMode::VpssRgb => {
                    let request_start = Instant::now();
                    let frame = vpss.as_mut().expect("VPSS pipeline exists").next(2_000)?;
                    let request_us = request_start.elapsed().as_micros() as u64;
                    let meta = frame.meta;
                    let camera_ion_us = frame.camera_request_us;
                    let vpss_wall_us = frame.vpss_wall_us;
                    let vpss_hardware_us = frame.vpss_hardware_us;
                    // SAFETY: the pipeline owns the destination ION allocation for
                    // the complete blocking runtime call. VPSS produced a 640x640
                    // RGB-planar frame with the 64-byte row alignment required by
                    // the runtime. An aligned model binds this frame directly;
                    // an ordinary model imports it through the runtime's TDMA path.
                    let detections = unsafe {
                        if aligned_input {
                            let frame_paddrs = [frame.physical_address];
                            model.infer_aligned_physical_timed(
                                AlignedPhysicalFrames {
                                    frame_paddrs: &frame_paddrs,
                                    pixel_format: PhysicalPixelFormat::RgbPlanar,
                                    source_width: 640,
                                    source_height: 480,
                                },
                                cli.config,
                                Some(&mut timing),
                            )?
                        } else if cli.verify_input && index == 0 {
                            model.infer_vpss_rgb_verified_timed(
                                frame.physical_address,
                                frame.rgb,
                                640,
                                480,
                                cli.config,
                                Some(&mut timing),
                            )?
                        } else {
                            model.infer_vpss_rgb_timed(
                                frame.physical_address,
                                640,
                                480,
                                cli.config,
                                Some(&mut timing),
                            )?
                        }
                    };
                    if cli.verify_input && index == 0 {
                        let mut cpu_timing = InferTiming::default();
                        let cpu_detections = model.infer_rgb_planar_timed(
                            frame.rgb,
                            640,
                            480,
                            cli.config,
                            Some(&mut cpu_timing),
                        )?;
                        println!(
                            "AKARS_TPU_INPUT_AB direct_detections={} cpu_detections={} \
                             cpu_copy_us={} cpu_forward_us={} cpu_postprocess_us={}",
                            detections.len(),
                            cpu_detections.len(),
                            cpu_timing.preprocess_us,
                            cpu_timing.forward_us,
                            cpu_timing.postprocess_us,
                        );
                        let software_yuv = PlanarYuvFrame {
                            data: frame.yuv.to_vec(),
                            width: frame.yuv_width,
                            height: frame.yuv_height,
                            format: frame.yuv_format,
                        };
                        let mut software_yuv_timing = InferTiming::default();
                        let software_yuv_detections = model.infer_yuv_timed(
                            &software_yuv,
                            cli.config,
                            Some(&mut software_yuv_timing),
                        )?;
                        model.compare_current_input_with_vpss(frame.rgb)?;
                        println!(
                            "AKARS_VPSS_CPU_AB vpss_detections={} software_yuv_detections={} \
                             software_preprocess_us={} software_forward_us={} \
                             software_postprocess_us={}",
                            detections.len(),
                            software_yuv_detections.len(),
                            software_yuv_timing.preprocess_us,
                            software_yuv_timing.forward_us,
                            software_yuv_timing.postprocess_us,
                        );
                    }
                    (
                        meta,
                        detections,
                        request_us,
                        camera_ion_us,
                        vpss_wall_us,
                        vpss_hardware_us,
                    )
                }
            };

        if index == 0 {
            first_sequence = meta.sequence;
        } else {
            skipped_sequences = skipped_sequences.saturating_add(
                meta.sequence
                    .saturating_sub(last_sequence)
                    .saturating_sub(1),
            );
        }
        last_sequence = meta.sequence;

        let total_us = total_start.elapsed().as_micros() as u64;
        if !detections.is_empty() {
            frames_with_detections += 1;
        }
        detections_total = detections_total.saturating_add(detections.len() as u64);

        totals.request_us = totals.request_us.saturating_add(request_us);
        totals.camera_ion_us = totals.camera_ion_us.saturating_add(camera_ion_us);
        totals.capture_us = totals
            .capture_us
            .saturating_add(meta.profile.frame_total_us);
        totals.preprocess_us = totals
            .preprocess_us
            .saturating_add(nonnegative_us(timing.preprocess_us));
        totals.forward_us = totals
            .forward_us
            .saturating_add(nonnegative_us(timing.forward_us));
        totals.postprocess_us = totals
            .postprocess_us
            .saturating_add(nonnegative_us(timing.postprocess_us));
        totals.total_us = totals.total_us.saturating_add(total_us);
        totals.max_total_us = totals.max_total_us.max(total_us);
        totals.vpss_wall_us = totals.vpss_wall_us.saturating_add(vpss_wall_us);
        totals.vpss_hardware_us = totals.vpss_hardware_us.saturating_add(vpss_hardware_us);
        totals.vpss_hardware_max_us = totals.vpss_hardware_max_us.max(vpss_hardware_us);

        let top_score_q10000 = if cli.report_frames || cli.timing_percentiles {
            detections
                .iter()
                .map(|detection| (detection.score * 10_000.0).round().max(0.0) as u32)
                .max()
                .unwrap_or(0)
        } else {
            0
        };
        if cli.report_frames {
            println!(
                "AKARS_LIVE_FRAME index={} sequence={} detections={} top_score_q10000={} \
                 request_us={} camera_ion_us={} capture_us={} preprocess_us={} forward_us={} \
                 postprocess_us={} vpss_wall_us={} vpss_hw_us={} total_us={}",
                index + 1,
                meta.sequence,
                detections.len(),
                top_score_q10000,
                request_us,
                camera_ion_us,
                meta.profile.frame_total_us,
                nonnegative_us(timing.preprocess_us),
                nonnegative_us(timing.forward_us),
                nonnegative_us(timing.postprocess_us),
                vpss_wall_us,
                vpss_hardware_us,
                total_us,
            );
        }
        if let Some(samples) = timing_samples.as_mut() {
            samples.push(FrameTiming {
                request_us,
                camera_ion_us,
                capture_us: meta.profile.frame_total_us,
                preprocess_us: nonnegative_us(timing.preprocess_us),
                forward_us: nonnegative_us(timing.forward_us),
                postprocess_us: nonnegative_us(timing.postprocess_us),
                vpss_wall_us,
                vpss_hardware_us,
                total_us,
                top_score_q10000,
            });
        }
        frames_processed = frames_processed
            .checked_add(1)
            .ok_or("processed frame count overflow")?;
    }

    let wall_us = wall_start.elapsed().as_micros() as u64;
    let stats = if let Some(camera) = camera.as_mut() {
        camera.stop()?;
        camera.stats()?
    } else {
        let vpss = vpss.as_mut().expect("VPSS pipeline exists");
        vpss.stop()?;
        vpss.stats()?
    };
    print_summary(
        frames_processed,
        cli.input,
        wall_us,
        &totals,
        first_sequence,
        last_sequence,
        skipped_sequences,
        frames_with_detections,
        detections_total,
        stats,
    );
    if let Some(samples) = timing_samples.as_deref() {
        print_timing_percentiles(samples);
        print_confidence_summary(samples);
    }
    println!(
        "AKARS_LIVE_TEST stop={} requested_seconds={} actual_us={} report_frames={} \
         timing_percentiles={}",
        if cli.duration_seconds.is_some() {
            "duration"
        } else {
            "frames"
        },
        cli.duration_seconds.unwrap_or(0),
        wall_us,
        u8::from(cli.report_frames),
        u8::from(cli.timing_percentiles),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn print_summary(
    frames: u32,
    input: InputMode,
    wall_us: u64,
    totals: &TimingTotals,
    first_sequence: u64,
    last_sequence: u64,
    skipped_sequences: u64,
    frames_with_detections: u64,
    detections_total: u64,
    stats: CameraCaptureStats,
) {
    let count = u64::from(frames);
    let fps_x100 = count
        .saturating_mul(100_000_000)
        .checked_div(wall_us)
        .unwrap_or(0);
    println!(
        "AKARS_LIVE_SUMMARY input={} frames={} wall_us={} fps_x100={} request_avg_us={} \
         camera_ion_avg_us={} capture_avg_us={} preprocess_avg_us={} forward_avg_us={} \
         postprocess_avg_us={} vpss_wall_avg_us={} vpss_hw_avg_us={} vpss_hw_max_us={} \
         total_avg_us={} total_max_us={}",
        input.name(),
        frames,
        wall_us,
        fps_x100,
        average(totals.request_us, count),
        average(totals.camera_ion_us, count),
        average(totals.capture_us, count),
        average(totals.preprocess_us, count),
        average(totals.forward_us, count),
        average(totals.postprocess_us, count),
        average(totals.vpss_wall_us, count),
        average(totals.vpss_hardware_us, count),
        totals.vpss_hardware_max_us,
        average(totals.total_us, count),
        totals.max_total_us,
    );
    println!(
        "AKARS_LIVE_RESULT first_sequence={} last_sequence={} skipped_sequences={} \
         frames_with_detections={} detections_total={}",
        first_sequence, last_sequence, skipped_sequences, frames_with_detections, detections_total,
    );
    println!(
        "AKARS_LIVE_CAMERA calls={} success={} failed={} retries={} invalid={} usb_errors={} \
         published={} overwritten={} avg_call_us={} max_frame_us={}",
        stats.capture_calls,
        stats.successful_frames,
        stats.failed_frames,
        stats.retry_attempts,
        stats.invalid_frames,
        stats.usb_errors,
        stats.published_frames,
        stats.overwritten_frames,
        average(stats.total_frame_us, stats.capture_calls),
        stats.max_frame_us,
    );
}

fn parse_cli(args: impl Iterator<Item = String>) -> Result<Cli, Box<dyn Error>> {
    let mut model = None;
    let mut device = PathBuf::from(DEFAULT_DEVICE);
    let mut vpss_device = PathBuf::from(DEFAULT_VPSS_DEVICE);
    let mut frames = DEFAULT_FRAMES;
    let mut duration_seconds = None;
    let mut report_frames = false;
    let mut timing_percentiles = false;
    let mut input = InputMode::VpssRgb;
    let mut verify_input = false;
    let mut config = InferenceConfig::default();
    let mut args = args.peekable();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "--device" => device = PathBuf::from(take_value(&mut args, "--device")?),
            "--vpss-device" => vpss_device = PathBuf::from(take_value(&mut args, "--vpss-device")?),
            "--frames" => frames = take_value(&mut args, "--frames")?.parse()?,
            "--duration-seconds" => {
                duration_seconds = Some(take_value(&mut args, "--duration-seconds")?.parse()?);
            }
            "--report-frames" => report_frames = true,
            "--timing-percentiles" => timing_percentiles = true,
            "--input" => input = InputMode::parse(&take_value(&mut args, "--input")?)?,
            "--verify-input" => verify_input = true,
            "--classes" => config.classes_num = take_value(&mut args, "--classes")?.parse()?,
            "--conf" => {
                config.confidence_threshold = take_value(&mut args, "--conf")?.parse()?;
            }
            "--iou" => config.iou_threshold = take_value(&mut args, "--iou")?.parse()?,
            value if value.starts_with('-') => {
                return Err(format!("unknown option: {value}").into());
            }
            value if model.is_none() => model = Some(PathBuf::from(value)),
            value => return Err(format!("unexpected argument: {value}").into()),
        }
    }
    if frames == 0 {
        return Err("--frames must be positive".into());
    }
    if duration_seconds == Some(0) {
        return Err("--duration-seconds must be positive".into());
    }
    if config.classes_num <= 0 {
        return Err("--classes must be positive".into());
    }
    Ok(Cli {
        model: model.ok_or("missing model path")?,
        device,
        vpss_device,
        frames,
        duration_seconds,
        report_frames,
        timing_percentiles,
        input,
        verify_input,
        config,
    })
}

fn take_value(
    args: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    option: &str,
) -> Result<String, Box<dyn Error>> {
    args.next()
        .ok_or_else(|| format!("{option} expects a value").into())
}

fn average(total: u64, count: u64) -> u64 {
    total.checked_div(count).unwrap_or(0)
}

fn nonnegative_us(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn print_timing_percentiles(samples: &[FrameTiming]) {
    let stages: [(&str, fn(&FrameTiming) -> u64); 9] = [
        ("request", |sample| sample.request_us),
        ("camera_ion", |sample| sample.camera_ion_us),
        ("capture", |sample| sample.capture_us),
        ("vpss_wall", |sample| sample.vpss_wall_us),
        ("vpss_hw", |sample| sample.vpss_hardware_us),
        ("preprocess", |sample| sample.preprocess_us),
        ("forward", |sample| sample.forward_us),
        ("postprocess", |sample| sample.postprocess_us),
        ("total", |sample| sample.total_us),
    ];
    for (stage, value) in stages {
        let mut values: Vec<u64> = samples.iter().map(value).collect();
        values.sort_unstable();
        let sum: u128 = values.iter().map(|value| u128::from(*value)).sum();
        let average = sum.checked_div(values.len() as u128).unwrap_or(0);
        println!(
            "AKARS_LIVE_TIMING stage={} avg_us={} p50_us={} p95_us={} p99_us={} max_us={}",
            stage,
            average,
            nearest_rank(&values, 50),
            nearest_rank(&values, 95),
            nearest_rank(&values, 99),
            values.last().copied().unwrap_or(0),
        );
    }
}

fn nearest_rank(sorted: &[u64], percentile: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = sorted.len().saturating_mul(percentile).saturating_add(99) / 100;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn print_confidence_summary(samples: &[FrameTiming]) {
    let scores: Vec<u32> = samples
        .iter()
        .map(|sample| sample.top_score_q10000)
        .filter(|score| *score > 0)
        .collect();
    let sum: u64 = scores.iter().map(|score| u64::from(*score)).sum();
    let mean = sum.checked_div(scores.len() as u64).unwrap_or(0);
    println!(
        "AKARS_LIVE_CONFIDENCE detected_frames={} mean_q10000={} min_q10000={} max_q10000={}",
        scores.len(),
        mean,
        scores.iter().min().copied().unwrap_or(0),
        scores.iter().max().copied().unwrap_or(0),
    );
}

fn print_usage() {
    eprintln!(
        "Usage: akars-tennis-live <model.cvimodel> [--device PATH] [--vpss-device PATH] [--frames \
         N | --duration-seconds N] [--report-frames] [--timing-percentiles] [--input \
         vpss-rgb|jpu-yuv|mjpeg] [--verify-input] [--classes N] [--conf X] [--iou X]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings<'a>(values: &'a [&'a str]) -> impl Iterator<Item = String> + 'a {
        values.iter().map(|value| (*value).to_owned())
    }

    #[test]
    fn parses_duration_and_per_frame_reporting() {
        let cli = parse_cli(strings(&[
            "model.cvimodel",
            "--duration-seconds",
            "60",
            "--report-frames",
            "--timing-percentiles",
        ]))
        .unwrap();
        assert_eq!(cli.duration_seconds, Some(60));
        assert!(cli.report_frames);
        assert!(cli.timing_percentiles);
    }

    #[test]
    fn rejects_zero_duration() {
        let error = parse_cli(strings(&["model.cvimodel", "--duration-seconds", "0"]))
            .err()
            .expect("zero duration must fail");
        assert!(error.to_string().contains("must be positive"));
    }

    #[test]
    fn nearest_rank_uses_ceiling_rank() {
        let values: Vec<u64> = (1..=100).collect();
        assert_eq!(nearest_rank(&values, 50), 50);
        assert_eq!(nearest_rank(&values, 95), 95);
        assert_eq!(nearest_rank(&values, 99), 99);
        assert_eq!(nearest_rank(&[7], 99), 7);
    }
}
