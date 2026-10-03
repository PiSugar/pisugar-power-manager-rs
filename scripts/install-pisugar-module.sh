#!/usr/bin/env bash

set -euo pipefail

channel=release
base_url="${PISUGAR_MODULE_BASE_URL:-}"

usage() {
    cat <<EOF
Install the prebuilt PiSugar desktop battery kernel module.

Usage: $0 [OPTIONS]

Options:
  -c, --channel CHANNEL  Download channel: release or nightly (default: release)
  -h, --help             Show this help

Set PISUGAR_MODULE_BASE_URL to download modules from a custom module archive.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -c|--channel) channel="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; exit 1 ;;
    esac
done

if ! { [[ -r /proc/device-tree/model ]] && tr -d '\0' < /proc/device-tree/model | grep -q 'Raspberry Pi'; } \
    && [[ "$(uname -r)" != *+rpt-rpi-* ]]; then
    echo "This installer only supports Raspberry Pi OS hardware." >&2
    exit 1
fi

case "$(uname -m)" in
    aarch64|arm64) module_arch=arm64 ;;
    armv6l|armv7l|armhf) module_arch=armhf ;;
    *) echo "Unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

kernel_release=$(uname -r)
kernel_version=${kernel_release%%+*}
case "$kernel_release" in
    6.1.0-rpi8-*) kernel_version=6.1.73 ;;
    # Raspberry Pi OS used ABI names such as 6.1.54-v8+ in 2023.
    # Historical module releases are grouped by the numeric kernel version.
    6.1.*-*) kernel_version=${kernel_release%%-*} ;;
esac
package="pisugar-module_${kernel_release}_${module_arch}.tar.gz"
if [[ -z "$base_url" ]]; then
    base_url="https://repo.pisugar.uk/PiSugar/pisugar-kernel-module/releases/download/kernel-${kernel_version}"
fi

temp_dir=$(mktemp -d /tmp/pisugar-module.XXXXXX)
trap 'rm -rf "$temp_dir"' EXIT

if curl -fL "$base_url/$package" -o "$temp_dir/$package"; then
    tar -xzf "$temp_dir/$package" -C "$temp_dir"
    bash "$temp_dir/install.sh" "$temp_dir/pisugar_battery.ko"
    exit 0
fi

echo "No prebuilt PiSugar module for ${kernel_release} (${module_arch}); building locally." >&2
if [[ ! -d "/lib/modules/${kernel_release}/build" ]]; then
    echo "Kernel headers are required for the local build: /lib/modules/${kernel_release}/build is missing." >&2
    exit 1
fi
command -v git >/dev/null || { echo "git is required for the local build." >&2; exit 1; }
command -v make >/dev/null || { echo "make is required for the local build." >&2; exit 1; }

git clone --depth 1 https://github.com/PiSugar/pisugar-power-manager-rs.git "$temp_dir/source"
make -C "$temp_dir/source/pisugar-module/pisugar-battery" KERNEL_DIR="/lib/modules/${kernel_release}/build"
bash "$temp_dir/source/pisugar-module/install-prebuilt.sh" \
    "$temp_dir/source/pisugar-module/pisugar-battery/pisugar_battery.ko"
