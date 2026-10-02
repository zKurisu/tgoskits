#!/usr/bin/env bash
set -euo pipefail

case_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
workspace_directory="$(cd -- "$case_directory/../../../.." && pwd)"
image_directory="${IMAGE_DIRECTORY:-/home/jiqingjie/pic}"
onnx_model="${ONNX_MODEL:-$workspace_directory/../AKA-00/tests/model/tennis.onnx}"
input_size="${INPUT_SIZE:-640}"
if [[ "$input_size" == 640 ]]; then
  default_model_name="yolov8n_tennis_aligned"
  default_work_directory="$case_directory/model-conversion/work"
else
  default_model_name="yolov8n_tennis_p2_${input_size}_aligned"
  default_work_directory="$case_directory/model-conversion/work-$input_size"
fi
model_name="${MODEL_NAME:-$default_model_name}"
work_directory="${WORK_DIRECTORY:-$default_work_directory}"
container_image="${TPU_MLIR_IMAGE:-akars/tpu-mlir:1.30.2}"
quantize_table="$case_directory/model-conversion/yolov8n-tennis-mixed.qtable"
formal_image_pattern="${FORMAL_IMAGE_PATTERN:-tennis_20261001_183102_*.jpg}"

if ! [[ "$input_size" =~ ^[0-9]+$ ]] \
  || ((input_size == 0 || input_size % 64 != 0)); then
  echo "error: INPUT_SIZE must be a positive multiple of 64, got: $input_size" >&2
  exit 1
fi

if [[ ! -f "$onnx_model" ]]; then
  echo "error: ONNX model not found: $onnx_model" >&2
  exit 1
fi
if [[ ! -d "$image_directory" ]]; then
  echo "error: calibration image directory not found: $image_directory" >&2
  exit 1
fi
if ! docker image inspect "$container_image" >/dev/null 2>&1; then
  echo "error: Docker image not found: $container_image" >&2
  echo "       build it with the model-conversion/Dockerfile first" >&2
  exit 1
fi

mapfile -t calibration_images < <(
  find "$image_directory" -maxdepth 1 -type f -name "$formal_image_pattern" -printf '%f\n' | sort
)
if ((${#calibration_images[@]} != 300)); then
  echo "error: expected exactly 300 formal calibration images, found ${#calibration_images[@]}" >&2
  exit 1
fi

mkdir -p -- "$work_directory"
rm -f -- "$work_directory/calibration-images.txt"
for image in "${calibration_images[@]}"; do
  printf '/data/%s\n' "$image" >>"$work_directory/calibration-images.txt"
done

test_image="/data/${calibration_images[149]}"
host_uid="$(id -u)"
host_gid="$(id -g)"

docker run --rm \
  --user "$host_uid:$host_gid" \
  -e HOME=/tmp \
  -e AKARS_INPUT_SIZE="$input_size" \
  -e AKARS_MODEL_NAME="$model_name" \
  -v "$onnx_model:/input/tennis.onnx:ro" \
  -v "$quantize_table:/input/yolov8n-tennis-mixed.qtable:ro" \
  -v "$image_directory:/data:ro" \
  -v "$work_directory:/work" \
  -w /work \
  "$container_image" \
  bash -lc '
    set -euo pipefail

    model_transform.py \
      --model_name "$AKARS_MODEL_NAME" \
      --model_def /input/tennis.onnx \
      --input_shapes "[[1,3,${AKARS_INPUT_SIZE},${AKARS_INPUT_SIZE}]]" \
      --output_names output0 \
      --pixel_format rgb \
      --channel_format nchw \
      --keep_aspect_ratio \
      --keep_ratio_mode letterbox \
      --pad_type center \
      --pad_value 0 \
      --mean 0,0,0 \
      --scale 0.00392156862745098,0.00392156862745098,0.00392156862745098 \
      --test_input '"$test_image"' \
      --test_result "${AKARS_MODEL_NAME}_top_outputs.npz" \
      --mlir "${AKARS_MODEL_NAME}.mlir"

    run_calibration.py "${AKARS_MODEL_NAME}.mlir" \
      --data_list calibration-images.txt \
      --input_num 300 \
      --chip cv181x \
      -o "${AKARS_MODEL_NAME}_cali_table"

    model_deploy.py \
      --mlir "${AKARS_MODEL_NAME}.mlir" \
      --quantize INT8 \
      --calibration_table "${AKARS_MODEL_NAME}_cali_table" \
      --quantize_table /input/yolov8n-tennis-mixed.qtable \
      --chip cv181x \
      --test_input '"$test_image"' \
      --test_reference "${AKARS_MODEL_NAME}_top_outputs.npz" \
      --compare_all \
      --tolerance 0.96,0.72 \
      --fuse_preprocess \
      --customization_format RGB_PLANAR \
      --aligned_input \
      --debug \
      --model "${AKARS_MODEL_NAME}_int8.cvimodel"
  '

sha256sum \
  "$work_directory/${model_name}_int8.cvimodel" \
  "$work_directory/${model_name}_cali_table" \
  "$onnx_model" \
  >"$work_directory/SHA256SUMS"

echo "conversion complete: $work_directory/${model_name}_int8.cvimodel"
