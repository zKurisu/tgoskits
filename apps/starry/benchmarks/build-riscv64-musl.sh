#!/usr/bin/env bash
# Cross-compile the StarryOS/Linux-shared micro-benchmarks for SG2002
# (riscv64 musl, static). The same static ELFs run unchanged on both Linux and
# StarryOS, so a single build yields a pair of comparison binaries.
#
# Toolchain selection (first match wins):
#   1. RISCV64_MUSL_CC        -- explicit compiler path/name
#   2. $AKARS_TENNIS_TOOLCHAIN_DIR/bin/riscv64-unknown-linux-musl-gcc
#      (reuses the Xuantie musl toolchain prepared by ../scripts/setup.sh)
#   3. riscv64-unknown-linux-musl-gcc on PATH
set -euo pipefail

bench_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
out_dir="${BENCH_OUT_DIR:-$bench_dir/install}"

cc=""
if [[ -n "${RISCV64_MUSL_CC:-}" ]]; then
    cc="$RISCV64_MUSL_CC"
elif [[ -n "${AKARS_TENNIS_TOOLCHAIN_DIR:-}" && -x "${AKARS_TENNIS_TOOLCHAIN_DIR}/bin/riscv64-unknown-linux-musl-gcc" ]]; then
    cc="${AKARS_TENNIS_TOOLCHAIN_DIR}/bin/riscv64-unknown-linux-musl-gcc"
elif command -v riscv64-unknown-linux-musl-gcc >/dev/null 2>&1; then
    cc="riscv64-unknown-linux-musl-gcc"
fi

if [[ -z "$cc" ]]; then
    echo "ERROR: riscv64 musl gcc not found." >&2
    echo "  Set RISCV64_MUSL_CC=/path/to/riscv64-unknown-linux-musl-gcc" >&2
    echo "  or run ../scripts/setup.sh and set AKARS_TENNIS_TOOLCHAIN_DIR." >&2
    exit 1
fi

mkdir -p "$out_dir"

echo "==> $cc syscost.c"
"$cc" -static -O2 -Wall -Wextra -o "$out_dir/syscost" "$bench_dir/syscost.c"

echo "==> $cc schbench.c"
"$cc" -static -O2 -Wall -Wextra -pthread -o "$out_dir/schbench" "$bench_dir/schbench.c"

echo "==> $cc syscall-latency.c"
"$cc" -static -O2 -Wall -Wextra -o "$out_dir/syscall-latency" "$bench_dir/syscall-latency.c"

echo "built:"
ls -l "$out_dir/syscost" "$out_dir/schbench" "$out_dir/syscall-latency"
