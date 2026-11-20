#!/usr/bin/env bash
# Update a PiSugar 3 through its platform-checked application OTA interface.
set -euo pipefail

standalone_target() {
    local machine bits
    machine=$(uname -m)
    bits=$(getconf LONG_BIT)
    case "$machine/$bits" in
        aarch64/64|arm64/64) echo aarch64-unknown-linux-musl ;;
        aarch64/32|arm64/32|armv[78]*/32) echo armv7-unknown-linux-musleabihf ;;
        armv6*/32) echo arm-unknown-linux-musleabi ;;
        *) echo "Unsupported Linux architecture: $machine ($bits-bit)" >&2; return 1 ;;
    esac
}

download() {
    local url=$1 destination=$2 expected=$3 attempt status
    for attempt in 1 2 3; do
        if curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' \
            --connect-timeout 15 --max-time 600 --output "$destination" "$url"; then
            break
        else
            status=$?
        fi
        if (( attempt == 3 )); then
            echo "Could not download ${url##*/}." >&2
            return "$status"
        fi
        echo "Retrying download of ${url##*/}..." >&2
        sleep "$attempt"
    done
    if ! printf '%s  %s\n' "$expected" "$destination" | sha256sum --check --status; then
        echo "SHA-256 verification failed for ${url##*/}." >&2
        return 1
    fi
}

cleanup() {
    local status=$?
    trap - EXIT
    trap '' HUP INT TERM
    if (( restore_service )); then
        if ! systemctl start pisugar-server; then
            echo 'Could not restart pisugar-server. Run sudo systemctl start pisugar-server.' >&2
            status=1
        fi
    fi
    if [[ -n "$work" ]]; then
        rm -rf -- "$work" || status=1
    fi
    if [[ -n "$install_tmp" ]]; then rm -f -- "$install_tmp" || status=1; fi
    exit "$status"
}

# The firmware version is an application register. Read it with SMBus byte-data
# transactions, using the same bus and address as the programmer's defaults.
plan_update() {
    python3 - <<'PY'
import ctypes
import fcntl
import re
import sys


class SMBusRequest(ctypes.Structure):
    _fields_ = [("direction", ctypes.c_uint8), ("command", ctypes.c_uint8),
                ("size", ctypes.c_uint32), ("data", ctypes.c_void_p)]


def sample(read):
    def header():
        return read(0x00), read(0x01)

    if header() != (3, 0x0f):
        raise ValueError("Expected a PiSugar 3 running its application. For bootloader recovery use the programmer directly.")
    marker = bytes(read(i) for i in range(0x52, 0x5a))
    # Original FM exposes all 16 serial bytes. Unmarked CH32 prototypes also
    # return a zero marker, but always zero-fill the last four serial bytes.
    serial = bytes(read(i) for i in range(0x10, 0x20)) if marker == bytes(8) else b""
    version = bytearray()
    for register in range(0xe2, 0xf0):
        byte = read(register)
        if byte == 0:
            break
        version.append(byte)
    else:
        raise ValueError("Firmware version has no terminator.")
    if header() != (3, 0x0f):
        raise ValueError("Device changed mode during the version check.")
    return marker, serial, bytes(version)


def decide(read):
    first = sample(read)
    if first != sample(read):
        raise ValueError("Inconsistent platform, serial or firmware version.")
    marker, serial, raw = first
    if not re.fullmatch(rb"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", raw):
        raise ValueError("Unrecognized application version: %r" % raw)
    installed = tuple(int(part) for part in raw[1:].split(b"."))
    if marker == bytes(8):
        if serial[-4:] == bytes(4) or serial[:12] in (bytes(12), b"\xff" * 12):
            raise ValueError("Unmarked board with an ambiguous serial number; verify the MCU before updating.")
        platform = "fm33lc0x"
    elif marker == bytes.fromhex("505349440101000e"):
        platform = "fm33lc0x"
    elif marker == bytes.fromhex("505349440102010c"):
        platform = "ch32l103"
    else:
        raise ValueError("Unknown platform identity: %s" % marker.hex())
    target = (1, 4, 2) if platform == "fm33lc0x" else (1, 4, 3)
    print("%s: installed %s, package v%d.%d.%d" % ((platform, raw.decode("ascii")) + target), file=sys.stderr)
    if installed > target:
        raise ValueError("Installed firmware is newer; downgrade refused.")
    if installed == target and marker == bytes(8):
        raise ValueError("The version matches but the expected platform marker is missing.")
    return ("current" if installed == target else "upgrade") + " " + platform


def main():
    with open("/dev/i2c-1", "rb+", buffering=0) as bus:
        fcntl.ioctl(bus, 0x0703, 0x57)  # I2C_SLAVE (without FORCE)
        data = ctypes.create_string_buffer(34)  # union i2c_smbus_data

        def read(register):
            req = SMBusRequest(1, register, 2, ctypes.addressof(data))
            fcntl.ioctl(bus, 0x0720, req)  # I2C_SMBUS_READ, BYTE_DATA
            return data.raw[0]

        print(decide(read))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError) as error:
        sys.exit(str(error))
PY
}

deb_status() {
    local rc
    dpkg-query -W -f='${Status}\t${Version}\t${Architecture}\n' "$1" && return 0
    rc=$?
    # Status 1 is also returned on first install when the package is absent.
    if (( rc == 1 )); then return 0; fi
    return "$rc"
}

install_programmer() {
    local status version architecture record visible
    if [[ -n "$deb" ]]; then
        record=$(deb_status pisugar-programmer)
        local already_installed=0
        while IFS=$'\t' read -r status version architecture; do
            if [[ -z "$version" || "$status" == *' config-files' || "$status" == *' not-installed' ]]; then continue; fi
            if dpkg --compare-versions "$version" gt "$package_version"; then
                echo "Installed Debian package $version is newer than $package_version; stopping." >&2
                return 1
            fi
            if [[ "$status" == *' ok installed' && "$version" == "$package_version" && "$architecture" == "$arch" ]]; then
                already_installed=1
            fi
        done <<< "$record"
        if (( ! already_installed )) || [[ ! -x "$installed_tool" ]] ||
                ! cmp -s "$tool_file" "$installed_tool"; then
            dpkg --refuse-downgrade --install "$deb"
        fi
        record=$(deb_status "pisugar-programmer:$arch")
        IFS=$'\t' read -r status version architecture <<< "$record"
        if [[ "$status" != *' ok installed' || "$version" != "$package_version" || "$architecture" != "$arch" ]]; then
            echo 'dpkg did not confirm the requested programmer package and architecture.' >&2
            return 1
        fi
    elif [[ ! -x "$installed_tool" ]] || ! cmp -s "$tool_file" "$installed_tool"; then
        for visible in "$installed_tool" /usr/bin/pisugar-programmer; do
            [[ -x "$visible" ]] || continue
            version=$("$visible" --version)
            if [[ ! "$version" =~ ^pisugar-programmer[[:space:]]([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
                echo "Could not identify the installed version at $visible." >&2
                return 1
            fi
            version=${BASH_REMATCH[1]}
            if [[ "$(printf '%s\n' "$version" 2.4.2 | sort -V | tail -1)" != 2.4.2 ]]; then
                echo "Existing programmer $version is newer than 2.4.2; stopping." >&2
                return 1
            fi
        done
        # Replacing a standalone installation by rename keeps its executable
        # available until the verified replacement is completely written.
        if [[ ! -d /usr/local/bin ]]; then install -d -m 0755 /usr/local/bin; fi
        install_tmp=$(mktemp /usr/local/bin/.pisugar-programmer.XXXXXXXX)
        install -m 0755 "$tool_file" "$install_tmp"
        mv -fT -- "$install_tmp" "$installed_tool"
        install_tmp=''
    fi
    hash -r
    visible=$(type -P pisugar-programmer || true)
    if ! cmp -s "$tool_file" "$installed_tool" || [[ "$("$installed_tool" --version)" != "$expected_version" ]]; then
        echo "Installed programmer verification failed at $installed_tool." >&2
        return 1
    fi
    if [[ -z "$visible" ]] || ! cmp -s "$visible" "$installed_tool"; then
        echo "Installed $installed_tool; PATH resolves pisugar-programmer to ${visible:-no executable}. Check PATH before updating." >&2
        return 1
    fi
    echo "Installed $expected_version at $installed_tool."
}

main() {
    local mode=update target tool_sha ota_url state version action platform attempt
    local arch='' deb='' tool_file installed_tool asset package_version=2.4.2-1
    local expected_version='pisugar-programmer 2.4.2'
    local release=https://github.com/PiSugar/pisugar-power-manager-rs/releases/download/pisugar-programmer-v2.4.2
    local ota_sha=f098d458b04e1294d3f78813d4686f10511ffe09e75cea1fb0349ff9a88d5c6d
    if (( $# == 1 )) && [[ "$1" == --check || "$1" == --install-only ]]; then
        mode=${1#--}
    elif (( $# )); then
        echo 'Usage: bash PiSugarUpdate.sh [--check | --install-only]' >&2
        return 2
    fi
    if [[ "$mode" != check && "$EUID" != 0 ]]; then
        echo 'Run the installation with sudo.' >&2
        return 1
    fi
    ota_url=https://cdn.pisugar.com/release/PiSugar3Firmware/combine/v1.4.3/pisugar-3-ota.zip

    umask 077
    work=''
    install_tmp=''
    restore_service=0
    trap cleanup EXIT
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
    work=$(mktemp -d /tmp/pisugar-update.XXXXXXXXXX)
    if command -v dpkg >/dev/null; then
        arch=$(dpkg --print-architecture)
        case "$arch" in
            arm64) tool_sha=a58dd54219f9543bebd6240a788917f5554eb46010a919097aacc7f6b1202ec8 ;;
            armhf) tool_sha=c415f0c46ea07e84930823539618eba702df8ea11bc2c421e7aeba8e4e15ed34 ;;
            armel) tool_sha=3a6ad5ac7842f4eafcae4c2bcdb07a0731988703600da2a72c051a7d6e8df697 ;;
            *) echo "Unsupported Debian architecture: $arch" >&2; return 1 ;;
        esac
        asset="pisugar-programmer_${package_version}_${arch}.deb"
        deb="$work/$asset"
        download "$release/$asset" "$deb" "$tool_sha"
        dpkg-deb -x "$deb" "$work/pkg"
        installed_tool=/usr/bin/pisugar-programmer
        tool_file="$work/pkg$installed_tool"
    else
        target=$(standalone_target)
        case "$target" in
            aarch64-unknown-linux-musl) tool_sha=709d0d39673b091d10a819dfac594bca78457e333dac254435b9266ca70452f3 ;;
            armv7-unknown-linux-musleabihf) tool_sha=8d4cb43cfc9f32caeed1b431bd82fb783bc5e3e0a6b6313fd3ac0545bb4068d0 ;;
            arm-unknown-linux-musleabi) tool_sha=5151fe034c819cc90ec631dabe004d6cdcf7b30e6a9981ec82924dc672cc8f09 ;;
        esac
        tool_file="$work/pisugar-programmer"
        installed_tool=/usr/local/bin/pisugar-programmer
        download "$release/pisugar-programmer-2.4.2-$target" "$tool_file" "$tool_sha"
        chmod 700 "$tool_file"
    fi
    version=$("$tool_file" --version)
    if [[ "$version" != "$expected_version" ]]; then
        echo "Unexpected programmer version: $version" >&2
        return 1
    fi
    if [[ "$mode" != install-only ]]; then
        download "$ota_url" "$work/pisugar-3-ota.zip" "$ota_sha"
        "$tool_file" --check "$work/pisugar-3-ota.zip"
    fi
    if [[ "$mode" == check ]]; then return 0; fi
    exec 9>>/run/pisugar-firmware-update.lock
    if ! flock -n 9; then
        echo 'Could not acquire the PiSugar firmware update lock.' >&2
        return 1
    fi
    install_programmer
    if [[ "$mode" == install-only ]]; then return 0; fi
    command -v python3 >/dev/null || { echo 'Python 3 is required to read the board version.' >&2; return 1; }
    if command -v systemctl >/dev/null; then
        state=$(systemctl is-active pisugar-server) || :
        case "$state" in
            active|reloading)
                restore_service=1
                systemctl stop pisugar-server ;;
            inactive|failed|unknown) ;;
            *) echo "Could not establish pisugar-server state: ${state:-unknown}." >&2; return 1 ;;
        esac
    elif command -v pisugar-server >/dev/null; then
        echo 'Cannot manage pisugar-server without systemctl.' >&2
        return 1
    fi
    if ! action=$(plan_update); then
        echo 'Could not verify the installed firmware version. Update cancelled.' >&2
        return 1
    fi
    case "$action" in
        'current fm33lc0x'|'current ch32l103')
            echo "The installed PiSugar firmware for ${action#current } is already current."
            return 0 ;;
        'upgrade fm33lc0x'|'upgrade ch32l103') platform=${action#upgrade } ;;
        *) echo 'Unrecognized board version check result.' >&2; return 1 ;;
    esac
    # Bind the write to the platform we just checked. The programmer independently
    # reads the board identity before reset and rejects a BIN for the other MCU.
    local image="$work/pisugar-3-$platform-application.bin"
    python3 -c '
from pathlib import Path
import sys
from zipfile import ZipFile
with ZipFile(sys.argv[1]) as bundle:
    Path(sys.argv[3]).write_bytes(bundle.read("pisugar-3-" + sys.argv[2] + "-application.bin"))
' "$work/pisugar-3-ota.zip" "$platform" "$image"
    echo 'Keep the Pi and PiSugar powered until the update finishes.'
    "$installed_tool" -r --yes "$image"
    for attempt in 1 2 3 4 5; do
        if action=$(plan_update) && [[ "$action" == "current $platform" ]]; then
            echo "PiSugar firmware update complete. Installed version verified for $platform."
            return 0
        fi
        if (( attempt < 5 )); then sleep 1; fi
    done
    echo 'The programmer finished, but the installed firmware version could not be confirmed.' >&2
    return 1
}

# Keep the call at EOF so piping a truncated function cannot start an update.
main "$@"
