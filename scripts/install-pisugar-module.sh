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
package="pisugar-module_${kernel_release}_${module_arch}.tar.gz"
if [[ -z "$base_url" ]]; then
    base_url="https://cdn.pisugar.com/${channel}"
fi

temp_dir=$(mktemp -d /tmp/pisugar-module.XXXXXX)
trap 'rm -rf "$temp_dir"' EXIT

if ! curl -fL "$base_url/$package" -o "$temp_dir/$package"; then
    echo "No prebuilt PiSugar module for ${kernel_release} (${module_arch})." >&2
    echo "Update Raspberry Pi OS to a supported kernel and run this installer again." >&2
    exit 1
fi

tar -xzf "$temp_dir/$package" -C "$temp_dir"
bash "$temp_dir/install.sh" "$temp_dir/pisugar_battery.ko"
