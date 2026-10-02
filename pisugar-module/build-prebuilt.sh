#!/usr/bin/env bash
set -euo pipefail

ARCH=${1:?Usage: build-prebuilt.sh <armhf|arm64> [output-directory]}
OUTPUT_DIR=${2:-dist}
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
SOURCE_DIR="$SCRIPT_DIR/pisugar-battery"

mkdir -p "$OUTPUT_DIR"
found=0

for kernel_dir in /lib/modules/*/build; do
    [[ -d "$kernel_dir" ]] || continue
    kernel_release=${kernel_dir#/lib/modules/}
    kernel_release=${kernel_release%/build}
    found=1

    make -C "$SOURCE_DIR" clean KERNEL_DIR="$kernel_dir"
    make -C "$SOURCE_DIR" KERNEL_DIR="$kernel_dir"

    staging=$(mktemp -d)
    install -m 644 "$SOURCE_DIR/pisugar_battery.ko" "$staging/pisugar_battery.ko"
    install -m 755 "$SCRIPT_DIR/install-prebuilt.sh" "$staging/install.sh"
    printf '%s\n' "$kernel_release" > "$staging/kernel-release"
    tar -C "$staging" -czf \
        "$OUTPUT_DIR/pisugar-module_${kernel_release}_${ARCH}.tar.gz" .
    rm -rf "$staging"
    make -C "$SOURCE_DIR" clean KERNEL_DIR="$kernel_dir"
done

if [[ "$found" -eq 0 ]]; then
    echo "No installed kernel build trees found under /lib/modules" >&2
    exit 1
fi
