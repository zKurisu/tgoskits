#!/usr/bin/env bash
set -Eeuo pipefail

# Run this script on the laptop. It keeps one SSH master connection open so the
# robot root password is requested only once. The robot stages at most one JPEG
# in /tmp; that file is removed only after SCP and local JPEG validation pass.

robot_host="${ROBOT_HOST:-192.168.86.53}"
robot_user="${ROBOT_USER:-root}"
script_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
case_directory="$(cd -- "$script_directory/.." && pwd)"
local_helper="${LOCAL_HELPER:-$case_directory/install/sg2002_riscv64_musl/akars_tennis/cvi-camera-dataset}"
remote_binary="${REMOTE_BINARY:-/tmp/akars-camera-dataset-tool}"
remote_device="${REMOTE_DEVICE:-/dev/cvi-usb-camera0}"
remote_ready="${REMOTE_READY:-/tmp/akars-camera-dataset.jpg}"
remote_pid_file="${REMOTE_PID_FILE:-/tmp/akars-camera-dataset.pid}"
local_directory="${LOCAL_DIRECTORY:-/home/jiqingjie/pic}"
frame_count="${FRAME_COUNT:-300}"
interval_ms="${INTERVAL_MS:-500}"

usage() {
  cat <<'EOF'
Usage: collect-camera-images.sh [ROBOT_IP [FRAME_COUNT [INTERVAL_MS [LOCAL_DIR]]]]

Defaults:
  ROBOT_IP    192.168.86.53
  FRAME_COUNT 300
  INTERVAL_MS 500
  LOCAL_DIR   /home/jiqingjie/pic

The script runs on the laptop and asks for the robot root password once.
It never stores the password. The locally built cvi-camera-dataset helper is
uploaded to /tmp on the robot and removed on exit. Environment variables
ROBOT_USER, LOCAL_HELPER, REMOTE_BINARY, REMOTE_DEVICE and REMOTE_READY can
override advanced settings.
EOF
}

is_positive_integer() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

jpeg_is_complete() {
  local image=$1
  local first_bytes last_bytes

  [[ -s "$image" ]] || return 1
  first_bytes="$(head -c 2 -- "$image" | od -An -tx1 | tr -d ' \n')"
  last_bytes="$(tail -c 2 -- "$image" | od -An -tx1 | tr -d ' \n')"
  [[ "$first_bytes" == "ffd8" && "$last_bytes" == "ffd9" ]]
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

robot_host="${1:-$robot_host}"
frame_count="${2:-$frame_count}"
interval_ms="${3:-$interval_ms}"
local_directory="${4:-$local_directory}"

if ! is_positive_integer "$frame_count" || ((frame_count > 100000)); then
  echo "error: FRAME_COUNT must be an integer between 1 and 100000" >&2
  exit 2
fi
if ! is_positive_integer "$interval_ms" || ((interval_ms > 3600000)); then
  echo "error: INTERVAL_MS must be an integer between 1 and 3600000" >&2
  exit 2
fi
if [[ ! -x "$local_helper" ]]; then
  echo "error: camera helper is not built: $local_helper" >&2
  echo "       run apps/starry/aka00-tennis-yolo/build-validator.sh first" >&2
  exit 2
fi
if [[ ! "$remote_binary" =~ ^/[A-Za-z0-9._/-]+$ ||
      ! "$remote_device" =~ ^/dev/[A-Za-z0-9._/-]+$ ||
      ! "$remote_ready" =~ ^/tmp/[A-Za-z0-9._/-]+$ ||
      ! "$remote_pid_file" =~ ^/tmp/[A-Za-z0-9._/-]+$ ]]; then
  echo "error: unsafe remote path configuration" >&2
  exit 2
fi

mkdir -p -- "$local_directory"
state_directory="$(mktemp -d /tmp/akars-camera-transfer.XXXXXX)"
control_socket="$state_directory/ssh-control"
status_fifo="$state_directory/status.fifo"
remote_log="$state_directory/remote.log"
mkfifo "$status_fifo"

ssh_target="${robot_user}@${robot_host}"
ssh_options=(
  -o "ControlPath=$control_socket"
  -o ConnectTimeout=10
  -o ServerAliveInterval=5
  -o ServerAliveCountMax=3
  -o StrictHostKeyChecking=accept-new
)
capture_job=""
master_started=0
uploaded_helper=0
completed=0

cleanup() {
  local exit_status=$?
  trap - EXIT INT TERM HUP

  if ((master_started)); then
    ssh "${ssh_options[@]}" "$ssh_target" \
      "if [ -f '$remote_pid_file' ]; then kill \"\$(cat '$remote_pid_file')\" 2>/dev/null || true; fi; rm -f '$remote_pid_file' '$remote_ready' '$remote_ready.part'; if [ '$uploaded_helper' = 1 ]; then rm -f '$remote_binary' '$remote_binary.upload'; fi" \
      >/dev/null 2>&1 || true
  fi
  if [[ -n "$capture_job" ]]; then
    kill "$capture_job" >/dev/null 2>&1 || true
    wait "$capture_job" >/dev/null 2>&1 || true
  fi
  if ((master_started)); then
    ssh "${ssh_options[@]}" -O exit "$ssh_target" >/dev/null 2>&1 || true
  fi
  rm -rf -- "$state_directory"

  if ((exit_status != 0)); then
    echo "FAILED after $completed/$frame_count images; no JPEG was intentionally retained on the robot." >&2
  fi
  exit "$exit_status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM HUP

echo "Connecting to $ssh_target (enter the robot root password once if prompted)..."
ssh -M -N -f "${ssh_options[@]}" -o ControlPersist=600 "$ssh_target"
master_started=1

uploaded_helper=1
scp -O -q -o "ControlPath=$control_socket" \
  "$local_helper" "$ssh_target:$remote_binary.upload"
ssh "${ssh_options[@]}" "$ssh_target" \
  "mv '$remote_binary.upload' '$remote_binary'; chmod 700 '$remote_binary'; test -c '$remote_device'" || {
    echo "error: helper deployment failed or $remote_device is unavailable" >&2
    exit 1
  }

run_id="$(date +%Y%m%d_%H%M%S)"
echo "Collecting $frame_count images every ${interval_ms} ms into $local_directory"

ssh "${ssh_options[@]}" "$ssh_target" \
  "umask 077; rm -f '$remote_ready' '$remote_ready.part' '$remote_pid_file'; echo \$\$ > '$remote_pid_file'; exec '$remote_binary' '$remote_device' '$frame_count' '$interval_ms' '$remote_ready'" \
  >"$status_fifo" 2>"$remote_log" &
capture_job=$!

exec 3<"$status_fifo"
while IFS= read -r status_line <&3; do
  echo "$status_line"
  [[ "$status_line" == CVI_CAMERA_DATASET_FRAME\ * ]] || continue

  frame_index=""
  for field in $status_line; do
    case "$field" in
      index=*) frame_index="${field#index=}" ;;
    esac
  done
  if ! is_positive_integer "$frame_index"; then
    echo "error: cannot parse frame index from: $status_line" >&2
    exit 1
  fi
  if ((frame_index != completed + 1)); then
    echo "error: expected frame index $((completed + 1)), got $frame_index" >&2
    exit 1
  fi

  printf -v padded_index '%04d' "$frame_index"
  final_image="$local_directory/tennis_${run_id}_${padded_index}.jpg"
  partial_image="$final_image.part"
  if [[ -e "$final_image" || -e "$partial_image" ]]; then
    echo "error: refusing to overwrite $final_image" >&2
    exit 1
  fi

  scp -O -q -o "ControlPath=$control_socket" \
    "$ssh_target:$remote_ready" "$partial_image"
  if ! jpeg_is_complete "$partial_image"; then
    rm -f -- "$partial_image"
    echo "error: SCP result is not a complete JPEG for frame $frame_index" >&2
    exit 1
  fi
  mv -- "$partial_image" "$final_image"

  # Removing the fixed spool file acknowledges successful SCP to the capture
  # process. Only then is another camera frame allowed to be published.
  ssh "${ssh_options[@]}" "$ssh_target" "rm -f '$remote_ready'"
  completed=$((completed + 1))
  echo "SAVED $completed/$frame_count $final_image"
done
exec 3<&-

if ! wait "$capture_job"; then
  capture_job=""
  if [[ -s "$remote_log" ]]; then
    cat "$remote_log" >&2
  fi
  exit 1
fi
capture_job=""

if ((completed != frame_count)); then
  [[ -s "$remote_log" ]] && cat "$remote_log" >&2
  echo "error: expected $frame_count images, received $completed" >&2
  exit 1
fi

ssh "${ssh_options[@]}" "$ssh_target" \
  "rm -f '$remote_pid_file' '$remote_ready' '$remote_ready.part' '$remote_binary' '$remote_binary.upload'"
uploaded_helper=0
echo "PASS: saved $completed JPEG images in $local_directory; robot spool is empty."
