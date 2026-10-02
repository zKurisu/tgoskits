#!/usr/bin/env bash
set -euo pipefail

case_dir="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/env.sh
source "$case_dir/scripts/env.sh"

if ! akars_tennis_toolchain_ready; then
  echo "error: Xuantie toolchain is missing: $AKARS_TENNIS_TOOLCHAIN_DIR" >&2
  echo "       run $case_dir/scripts/setup.sh or set AKARS_TENNIS_TOOLCHAIN_DIR" >&2
  exit 1
fi

if ! akars_tennis_tpu_sdk_ready; then
  echo "error: TPU runtime libs are missing: $AKARS_TPU_SDK_DIR" >&2
  exit 1
fi

export AKARS_TPU_SDK_DIR
export PATH="$AKARS_TENNIS_TOOLCHAIN_DIR/bin:$PATH"
export CC="${CC:-$AKARS_TENNIS_CC}"
export CXX="${CXX:-$AKARS_TENNIS_CXX}"
export AR="${AR:-$AKARS_TENNIS_AR}"
export CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_MUSL_LINKER="$case_dir/scripts/linker.sh"

src_dir="$case_dir/akars-validator"
install_dir="$case_dir/install/sg2002_riscv64_musl/akars_tennis"

(
  cd "$src_dir"
  cargo build --release --target "$AKARS_TENNIS_TARGET"
)

rm -rf "$install_dir"
mkdir -p "$install_dir/model" "$install_dir/validation" "$install_dir/lib"

install -m 0755 \
  "$src_dir/target/$AKARS_TENNIS_TARGET/release/akars-tennis-validator" \
  "$install_dir/akars-tennis-validator"
install -m 0755 \
  "$src_dir/target/$AKARS_TENNIS_TARGET/release/akars-tennis-live" \
  "$install_dir/akars-tennis-live"
"$AKARS_TENNIS_CC" \
  -std=c11 -O2 -Wall -Wextra -Werror \
  -I"$case_dir/include" \
  "$case_dir/tools/cvi-camera-bench.c" \
  -o "$install_dir/cvi-camera-bench"
"$AKARS_TENNIS_CC" \
  -std=c11 -O2 -Wall -Wextra -Werror \
  -I"$case_dir/include" \
  "$case_dir/tools/cvi-camera-dataset.c" \
  -o "$install_dir/cvi-camera-dataset"
"$AKARS_TENNIS_CC" \
  -std=c11 -O2 -Wall -Wextra -Werror \
  -I"$case_dir/include" \
  "$case_dir/tools/cvi-vpss-smoke.c" \
  -o "$install_dir/cvi-vpss-smoke"
install -m 0644 \
  "$case_dir/model/yolov8n_tennis_v2.cvimodel" \
  "$case_dir/model/yolov8n_tennis_p2_aligned_int8.cvimodel" \
  "$case_dir/model/yolov8n_tennis_p2_384_aligned_int8.cvimodel" \
  "$install_dir/model/"
install -m 0644 "$case_dir/validation/"*.jpg "$case_dir/validation/images.txt" "$case_dir/validation/expected.txt" "$install_dir/validation/"
install -m 0644 "$AKARS_TPU_SDK_DIR/lib/"*.so* "$install_dir/lib/"

for lib in libstdc++.so.6 libgcc_s.so.1 libc.so; do
  candidate="$("$AKARS_TENNIS_CC" -print-file-name="$lib")"
  if [[ -f "$candidate" ]]; then
    install -m 0644 "$candidate" "$install_dir/lib/"
  fi
done

cat > "$install_dir/run.sh" <<'RUN_SH'
#!/bin/sh
set -eu

load_tpu_drivers() {
  [ -e /dev/cvi-tpu0 ] && return 0

  for module in cv181x_sys cv181x_base cv181x_tpu; do
    if ! grep -q "^${module} " /proc/modules 2>/dev/null; then
      insmod "/mnt/system/ko/${module}.ko"
    fi
  done
}

load_tpu_drivers

cd /akars_tennis
export LD_LIBRARY_PATH=/akars_tennis/lib:${LD_LIBRARY_PATH:-}

./akars-tennis-validator \
  model/yolov8n_tennis_v2.cvimodel \
  validation/images.txt \
  validation/expected.txt \
  --classes 1 \
  --conf 0.5 \
  --iou 0.5

echo STARRY_AKA00_TENNIS_DETECT_OK
RUN_SH
chmod 0755 "$install_dir/run.sh"

cat > "$install_dir/run-live.sh" <<'RUN_LIVE_SH'
#!/bin/sh
set -eu

load_tpu_drivers() {
  [ -e /dev/cvi-tpu0 ] && return 0

  for module in cv181x_sys cv181x_base cv181x_tpu; do
    if ! grep -q "^${module} " /proc/modules 2>/dev/null; then
      insmod "/mnt/system/ko/${module}.ko"
    fi
  done
}

frames="${1:-100}"
input="${2:-vpss-rgb}"
model="${AKARS_MODEL:-model/yolov8n_tennis_v2.cvimodel}"
verify_arg=""
if [ "${3:-}" = "verify" ]; then
  verify_arg="--verify-input"
fi
load_tpu_drivers

cd /akars_tennis
export LD_LIBRARY_PATH=/akars_tennis/lib:${LD_LIBRARY_PATH:-}

exec ./akars-tennis-live \
  "$model" \
  --device /dev/cvi-usb-camera0 \
  --vpss-device /dev/cvi-vpss0 \
  --frames "$frames" \
  --input "$input" \
  $verify_arg \
  --classes 1 \
  --conf 0.5 \
  --iou 0.5
RUN_LIVE_SH
chmod 0755 "$install_dir/run-live.sh"

install -m 0755 \
  "$case_dir/scripts/test-live-accuracy.sh" \
  "$install_dir/test-live-accuracy.sh"

echo "installed: $install_dir"
