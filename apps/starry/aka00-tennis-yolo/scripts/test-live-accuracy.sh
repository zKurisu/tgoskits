#!/bin/sh
set -eu

usage() {
  cat <<'EOF'
Usage: ./test-live-accuracy.sh [duration_seconds] [log_path]

Environment overrides:
  AKARS_MODEL   CVI model path (default: validated 640x640 model)
  AKARS_INPUT   vpss-rgb, jpu-yuv, or mjpeg (default: vpss-rgb)
  AKARS_CONF    confidence threshold (default: 0.5)
  AKARS_IOU     NMS IoU threshold (default: 0.5)
EOF
}

case "${1:-}" in
  -h|--help)
    usage
    exit 0
    ;;
esac

duration_seconds="${1:-60}"
case "$duration_seconds" in
  ''|*[!0-9]*)
    echo "error: duration_seconds must be a positive integer" >&2
    exit 2
    ;;
esac
if [ "$duration_seconds" -eq 0 ]; then
  echo "error: duration_seconds must be positive" >&2
  exit 2
fi

case_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
timestamp=$(date +%Y%m%d-%H%M%S 2>/dev/null || echo current)
log_path="${2:-/root/akars-live-accuracy-${timestamp}.log}"
model="${AKARS_MODEL:-model/yolov8n_tennis_v2.cvimodel}"
input="${AKARS_INPUT:-vpss-rgb}"
conf="${AKARS_CONF:-0.5}"
iou="${AKARS_IOU:-0.5}"

load_tpu_drivers() {
  [ -e /dev/cvi-tpu0 ] && return 0

  for module in cv181x_sys cv181x_base cv181x_tpu; do
    if ! grep -q "^${module} " /proc/modules 2>/dev/null; then
      insmod "/mnt/system/ko/${module}.ko"
    fi
  done
}

extract_field() {
  prefix=$1
  key=$2
  awk -v prefix="$prefix" -v key="$key" '
    $1 == prefix {
      for (i = 2; i <= NF; i++) {
        split($i, pair, "=")
        if (pair[1] == key) value = pair[2]
      }
    }
    END {
      if (value == "") exit 1
      print value
    }
  ' "$log_path"
}

load_tpu_drivers
cd "$case_dir"
export LD_LIBRARY_PATH="$case_dir/lib:${LD_LIBRARY_PATH:-}"

echo "AKARS_ACCURACY_START duration_seconds=$duration_seconds model=$model input=$input conf=$conf iou=$iou"
set +e
./akars-tennis-live \
  "$model" \
  --device /dev/cvi-usb-camera0 \
  --vpss-device /dev/cvi-vpss0 \
  --duration-seconds "$duration_seconds" \
  --timing-percentiles \
  --input "$input" \
  --classes 1 \
  --conf "$conf" \
  --iou "$iou" \
  > "$log_path" 2>&1
status=$?
set -e
if [ "$status" -ne 0 ]; then
  cat "$log_path" >&2
  echo "AKARS_ACCURACY_FAIL status=$status log=$log_path" >&2
  exit "$status"
fi

frames=$(extract_field AKARS_LIVE_SUMMARY frames)
wall_us=$(extract_field AKARS_LIVE_SUMMARY wall_us)
fps_x100=$(extract_field AKARS_LIVE_SUMMARY fps_x100)
detected_frames=$(extract_field AKARS_LIVE_RESULT frames_with_detections)
detections_total=$(extract_field AKARS_LIVE_RESULT detections_total)
skipped_sequences=$(extract_field AKARS_LIVE_RESULT skipped_sequences)
failed=$(extract_field AKARS_LIVE_CAMERA failed)
retries=$(extract_field AKARS_LIVE_CAMERA retries)
invalid=$(extract_field AKARS_LIVE_CAMERA invalid)
usb_errors=$(extract_field AKARS_LIVE_CAMERA usb_errors)
missed_frames=$((frames - detected_frames))
detection_rate=$(awk -v hit="$detected_frames" -v total="$frames" \
  'BEGIN { if (total == 0) print "0.00"; else printf "%.2f", hit * 100.0 / total }')
fps=$(awk -v value="$fps_x100" 'BEGIN { printf "%.2f", value / 100.0 }')
actual_seconds=$(awk -v value="$wall_us" 'BEGIN { printf "%.3f", value / 1000000.0 }')

camera_jpu_vpss_us=$(extract_field AKARS_LIVE_SUMMARY request_avg_us)
camera_ion_us=$(extract_field AKARS_LIVE_SUMMARY camera_ion_avg_us)
jpu_decode_us=$(extract_field AKARS_LIVE_SUMMARY jpu_decode_avg_us)
vpss_wall_us=$(extract_field AKARS_LIVE_SUMMARY vpss_wall_avg_us)
vpss_hw_us=$(extract_field AKARS_LIVE_SUMMARY vpss_hw_avg_us)
tdma_us=$(extract_field AKARS_LIVE_SUMMARY preprocess_avg_us)
inference_us=$(extract_field AKARS_LIVE_SUMMARY forward_avg_us)
postprocess_us=$(extract_field AKARS_LIVE_SUMMARY postprocess_avg_us)
total_us=$(extract_field AKARS_LIVE_SUMMARY total_avg_us)
other_us=$((total_us - camera_jpu_vpss_us - tdma_us - inference_us - postprocess_us))
if [ "$other_us" -lt 0 ]; then
  other_us=0
fi
camera_wait_us=$((camera_ion_us - jpu_decode_us))
if [ "$camera_wait_us" -lt 0 ]; then
  camera_wait_us=0
fi
camera_jpu_vpss_ms=$(awk -v value="$camera_jpu_vpss_us" 'BEGIN { printf "%.3f", value / 1000.0 }')
camera_wait_ms=$(awk -v value="$camera_wait_us" 'BEGIN { printf "%.3f", value / 1000.0 }')
jpu_decode_ms=$(awk -v value="$jpu_decode_us" 'BEGIN { printf "%.3f", value / 1000.0 }')
vpss_wall_ms=$(awk -v value="$vpss_wall_us" 'BEGIN { printf "%.3f", value / 1000.0 }')
vpss_hw_ms=$(awk -v value="$vpss_hw_us" 'BEGIN { printf "%.3f", value / 1000.0 }')
tdma_ms=$(awk -v value="$tdma_us" 'BEGIN { printf "%.3f", value / 1000.0 }')
inference_ms=$(awk -v value="$inference_us" 'BEGIN { printf "%.3f", value / 1000.0 }')
postprocess_ms=$(awk -v value="$postprocess_us" 'BEGIN { printf "%.3f", value / 1000.0 }')
other_ms=$(awk -v value="$other_us" 'BEGIN { printf "%.3f", value / 1000.0 }')
total_ms=$(awk -v value="$total_us" 'BEGIN { printf "%.3f", value / 1000.0 }')

score_mean_q10000=$(extract_field AKARS_LIVE_CONFIDENCE mean_q10000)
score_min_q10000=$(extract_field AKARS_LIVE_CONFIDENCE min_q10000)
score_max_q10000=$(extract_field AKARS_LIVE_CONFIDENCE max_q10000)
score_mean=$(awk -v value="$score_mean_q10000" 'BEGIN { printf "%.2f", value / 100.0 }')
score_min=$(awk -v value="$score_min_q10000" 'BEGIN { printf "%.2f", value / 100.0 }')
score_max=$(awk -v value="$score_max_q10000" 'BEGIN { printf "%.2f", value / 100.0 }')

grep -E '^(AKARS_TPU_INPUT|AKARS_VPSS_BUFFERS|AKARS_LIVE_SUMMARY|AKARS_LIVE_RESULT|AKARS_LIVE_CAMERA|AKARS_LIVE_TIMING|AKARS_LIVE_CONFIDENCE|AKARS_LIVE_TEST)' "$log_path"
{
  printf 'AKARS_ACCURACY_RESULT requested_seconds=%s actual_seconds=%s frames=%s fps=%s detected_frames=%s missed_frames=%s positive_detection_rate_percent=%s detections_total=%s score_mean_percent=%s score_min_percent=%s score_max_percent=%s\n' \
    "$duration_seconds" "$actual_seconds" "$frames" "$fps" "$detected_frames" "$missed_frames" \
    "$detection_rate" "$detections_total" "$score_mean" "$score_min" "$score_max"
  printf 'AKARS_ACCURACY_HEALTH skipped_sequences=%s camera_failed=%s retries=%s invalid=%s usb_errors=%s\n' \
    "$skipped_sequences" "$failed" "$retries" "$invalid" "$usb_errors"
  printf 'AKARS_PER_FRAME_AVERAGE camera_jpu_vpss_us=%s camera_wait_us=%s jpu_decode_us=%s vpss_wall_us=%s vpss_hw_irq_us=%s tdma_us=%s inference_us=%s postprocess_us=%s other_us=%s total_us=%s confidence_percent=%s detection_rate_percent=%s\n' \
    "$camera_jpu_vpss_us" "$camera_wait_us" "$jpu_decode_us" "$vpss_wall_us" "$vpss_hw_us" \
    "$tdma_us" "$inference_us" "$postprocess_us" "$other_us" "$total_us" "$score_mean" "$detection_rate"
  echo 'AKARS_ACCURACY_NOTE metric=positive_frame_detection_rate ground_truth=0 negative_samples=0 formal_map=unavailable'
  printf 'AKARS_ACCURACY_LOG path=%s\n' "$log_path"
  echo
  echo '单帧平均性能汇总'
  printf '摄像头/JPU等待 + VPSS     %10s ms\n' "$camera_jpu_vpss_ms"
  printf '  摄像头等待及IOCTL       %10s ms\n' "$camera_wait_ms"
  printf '  JPU硬件解码             %10s ms\n' "$jpu_decode_ms"
  printf '  VPSS墙钟（已含首行）    %10s ms\n' "$vpss_wall_ms"
  printf '  VPSS硬件IRQ             %10s ms\n' "$vpss_hw_ms"
  printf 'TPU输入TDMA搬运           %10s ms\n' "$tdma_ms"
  printf 'TPU推理（Forward墙钟）    %10s ms\n' "$inference_ms"
  printf '后处理                    %10s ms\n' "$postprocess_ms"
  printf '其他绑定/统计开销         %10s ms\n' "$other_ms"
  echo '----------------------------------------'
  printf '总时延                    %10s ms\n' "$total_ms"
  printf '单帧平均置信度            %10s %%\n' "$score_mean"
  printf '含球帧检出率              %10s %%\n' "$detection_rate"
} | tee -a "$log_path"
