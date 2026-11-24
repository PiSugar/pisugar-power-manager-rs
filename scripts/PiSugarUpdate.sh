#!/usr/bin/env bash
# Update a PiSugar 3 through its platform-checked application OTA interface.
set -euo pipefail

# Only this reporter runs in the background. Downloads, dpkg and all board
# operations stay in the foreground and keep their normal output and exit status.
watch_progress() {
    local label='' file='' total=0 started=$SECONDS next_label next_file next_total rc bytes
    trap '' HUP INT TERM
    while :; do
        if IFS='|' read -r -t 5 next_label next_file next_total; then
            [[ -n "$next_label" ]] || return 0
            label=$next_label file=$next_file total=$next_total started=$SECONDS
        else
            rc=$?
            (( rc > 128 )) || return 0  # EOF when the updater exits.
            [[ -n "$label" && "$total" != -1 ]] || continue
            if [[ -n "$file" ]]; then
                bytes=0
                if [[ -f "$file" ]]; then bytes=$(wc -c < "$file") || bytes=0; fi
                printf '[PiSugar] %s — %d/%d bytes (%d%%), %ds elapsed\n' \
                    "$label" "$bytes" "$total" "$((100 * bytes / total))" "$((SECONDS - started))" >&2
            else
                printf '[PiSugar] %s — %ds elapsed\n' "$label" "$((SECONDS - started))" >&2
            fi
        fi
    done
}

step() {
    current_step=$1
    printf '[PiSugar] %s\n' "$current_step" >&2
    (printf '%s|%s|%s\n' "$current_step" "${2:-}" "${3:-0}" >&8) 2>/dev/null || :
}

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
    local url=$1 destination=$2 expected=$3 total=$4 attempt status host=${1#https://}
    host=${host%%/*}
    for attempt in 1 2 3; do
        : > "$destination"
        step "Downloading ${url##*/} from $host (attempt $attempt/3)" "$destination" "$total"
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
    step "Checking SHA-256 of ${url##*/}"
    if ! printf '%s  %s\n' "$expected" "$destination" | sha256sum --check --status; then
        echo "SHA-256 verification failed for ${url##*/}." >&2
        return 1
    fi
    printf '[PiSugar] Download verified: %s (%d/%d bytes, 100%%)\n' "${url##*/}" "$total" "$total" >&2
}

cleanup() {
    local status=$?
    trap - EXIT
    trap '' HUP INT TERM
    if (( status != 0 )); then
        printf '[PiSugar] Failed during "%s" (exit %d).\n' "$current_step" "$status" >&2
    fi
    if (( restore_service )); then
        step 'Restarting pisugar-server'
        if ! systemctl start pisugar-server; then
            echo 'Could not restart pisugar-server. Run sudo systemctl start pisugar-server.' >&2
            status=1
        fi
    fi
    step 'Cleaning up temporary files'
    if [[ -n "$work" ]]; then
        rm -rf -- "$work" || status=1
    fi
    if [[ -n "$install_tmp" ]]; then rm -f -- "$install_tmp" || status=1; fi
    (printf '\n' >&8) 2>/dev/null || :
    exec 8>&-
    wait "$reporter_pid" || :
    if (( status == 0 )); then
        printf '[PiSugar] %s (%ds total).\n' "$finish_message" "$((SECONDS - run_started))" >&2
    else
        printf '[PiSugar] %s stopped (exit %d, %ds total).\n' "$operation" "$status" "$((SECONDS - run_started))" >&2
    fi
    exit "$status"
}

# The firmware version is an application register. Read it with SMBus byte-data
# transactions, using the same bus and address as the programmer's defaults.
plan_update() {
    python3 - "${1:-probe}" <<'PY'
import ctypes
import errno
import fcntl
import re
import sys
import time


class SMBusRequest(ctypes.Structure):
    _fields_ = [("direction", ctypes.c_uint8), ("command", ctypes.c_uint8),
                ("size", ctypes.c_uint32), ("data", ctypes.c_void_p)]


class BoardNotReady(ValueError):
    """The board has not settled into a readable firmware mode."""


def sample(read):
    def header():
        return read(0x00), read(0x01)

    product, mode = header()
    if product in (0, 0xff) or product == 3 and mode in (0, 0xff):
        raise BoardNotReady("Waiting for the PiSugar registers to initialize.")
    if product != 3 or mode not in (0x0f, 0xf0, 0xba):
        raise ValueError("Unrecognized product or mode on the PiSugar I2C address.")
    marker = bytes(read(i) for i in range(0x52, 0x5a))
    if mode != 0x0f:
        if header() != (product, mode):
            raise BoardNotReady("Device changed mode during the bootloader check.")
        return mode, marker, b"", None
    # Original FM exposes all 16 serial bytes. Unmarked CH32 prototypes also
    # return a zero marker, but always zero-fill the last four serial bytes.
    serial = bytes(read(i) for i in range(0x10, 0x20)) if marker == bytes(8) else b""
    version = bytearray()
    sampled_version = None
    try:
        for register in range(0xe2, 0xf0):
            byte = read(register)
            if byte == 0:
                break
            version.append(byte)
        else:
            # Preserve the bytes for the second independent sample.
            sampled_version = ("unterminated", bytes(version))
    except OSError as error:
        # Only an explicit platform ID can qualify for application repair.
        # Record the failure point/code as part of the stability check.
        if marker == bytes(8) or error.errno not in (errno.ENXIO, 121):  # Linux EREMOTEIO
            raise
        sampled_version = ("read_error", bytes(version), register, error.errno)
    if header() != (product, mode):
        raise BoardNotReady("Device changed mode during the version check.")
    return mode, marker, serial, sampled_version if sampled_version is not None else bytes(version)


def decide(read):
    first = sample(read)
    second = sample(read)
    if first[0] != second[0]:
        raise BoardNotReady("Device changed mode between identity checks.")
    if first != second:
        raise ValueError("Inconsistent platform, serial or firmware version.")
    mode, marker, serial, raw = first
    if marker == bytes(8):
        if mode == 0xf0:
            print("Legacy FM bootloader detected (03 F0, zero platform marker).", file=sys.stderr)
            return "recover fm33lc0x bootloader"
        if mode != 0x0f:
            raise ValueError("Cannot identify an unmarked bootapp.")
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
    if mode != 0x0f:
        if platform != "ch32l103":
            raise ValueError("Unexpected FM identity in bootloader mode.")
        stage = "bootloader" if mode == 0xf0 else "bootapp"
        print("%s %s: installed application version unavailable; package v%d.%d.%d" %
              ((platform, stage) + target), file=sys.stderr)
        return "recover %s %s" % (platform, stage)
    if not raw or isinstance(raw, tuple):
        if marker == bytes(8):
            raise ValueError("Cannot verify the installed version of this unmarked board.")
        print("%s application version unreadable; package v%d.%d.%d" % ((platform,) + target),
              file=sys.stderr)
        return "recover %s unreadable" % platform
    if not re.fullmatch(rb"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", raw):
        raise ValueError("Unrecognized application version: %r" % raw)
    installed = tuple(int(part) for part in raw[1:].split(b"."))
    print("%s: installed %s, package v%d.%d.%d" % ((platform, raw.decode("ascii")) + target), file=sys.stderr)
    if installed > target:
        if marker == bytes(8):
            raise ValueError("Unexpected newer version without the FM platform marker.")
        return "downgrade %s %s" % (platform, raw.decode("ascii"))
    if installed == target and marker == bytes(8):
        raise ValueError("The version matches but the expected platform marker is missing.")
    return ("current" if installed == target else "upgrade") + " " + platform


def wait_for_board(read, sleep):
    waiting = False
    while True:
        try:
            return decide(read)
        except BoardNotReady:
            pass
        except OSError as error:
            if error.errno not in (errno.EIO, errno.ENXIO, 121, errno.ETIMEDOUT):
                raise
        if not waiting:
            print("[PiSugar] Waiting for the board. Press and release Reset; Ctrl-C cancels.",
                  file=sys.stderr, flush=True)
            waiting = True
        sleep(0.1)


def main():
    with open("/dev/i2c-1", "rb+", buffering=0) as bus:
        fcntl.ioctl(bus, 0x0703, 0x57)  # I2C_SLAVE (without FORCE)
        data = ctypes.create_string_buffer(34)  # union i2c_smbus_data

        def read(register):
            req = SMBusRequest(1, register, 2, ctypes.addressof(data))
            fcntl.ioctl(bus, 0x0720, req)  # I2C_SMBUS_READ, BYTE_DATA
            return data.raw[0]

        print(wait_for_board(read, time.sleep) if sys.argv[1] == "wait-board" else decide(read))


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(130)
    except (OSError, ValueError) as error:
        sys.exit(str(error))
PY
}

confirm_flash() {
    local question=$1 reply=''
    step "Waiting for confirmation: $question" '' -1
    if { exec 7<>/dev/tty; } 2>/dev/null; then
        printf '%s [y/N] ' "$question" >&7
        IFS= read -r reply <&7 || reply=''
        exec 7>&-
        case "$reply" in [yY]|[yY][eE][sS]) return 0 ;; esac
    else
        printf '[PiSugar] No interactive terminal is available for confirmation.\n' >&2
    fi
    finish_message='Firmware flash skipped; no firmware was written.'
    return 1
}

prepare_i2c() {
    local state
    step 'Checking pisugar-server before accessing I2C'
    if command -v systemctl >/dev/null; then
        state=$(systemctl is-active pisugar-server) || :
        case "$state" in
            active|reloading)
                restore_service=1
                step 'Stopping pisugar-server before accessing I2C'
                systemctl stop pisugar-server ;;
            inactive|failed|unknown) printf '[PiSugar] pisugar-server status: %s.\n' "$state" >&2 ;;
            *) echo "Could not establish pisugar-server state: ${state:-unknown}." >&2; return 1 ;;
        esac
    elif command -v pisugar-server >/dev/null; then
        echo 'Cannot manage pisugar-server without systemctl.' >&2
        return 1
    fi
}

deb_status() {
    local rc
    dpkg-query -W -f='${Status}\t${Version}\t${Architecture}\n' "$1" && return 0
    rc=$?
    # Status 1 is also returned on first install when the package is absent.
    if (( rc == 1 )); then return 0; fi
    return "$rc"
}

stage_installed_programmer() {
    local status version architecture record
    [[ -f "$installed_tool" && -x "$installed_tool" ]] || return 1
    if [[ -n "$arch" ]]; then
        record=$(deb_status "pisugar-programmer:$arch") || return 1
        IFS=$'\t' read -r status version architecture <<< "$record"
        [[ "$status" == *' ok installed' && "$version" == "$package_version" && "$architecture" == "$arch" ]] || return 1
    fi
    # Use the same verified bytes for offline checks and the post-install comparison,
    # even if another process replaces the installed file during a download.
    cp -- "$installed_tool" "$work/local-programmer" || return 1
    printf '%s  %s\n' "$binary_sha" "$work/local-programmer" | sha256sum --check --status
}

install_programmer() {
    local status version architecture record visible
    if [[ -n "$deb" ]]; then
        step 'Checking the installed Debian programmer package'
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
            if [[ ! -f "$deb" ]]; then
                # The installation changed after we verified the local executable.
                download "$release/$asset" "$deb" "$tool_sha" "$tool_bytes"
            fi
            step "Installing pisugar-programmer $package_version ($arch) with dpkg"
            dpkg --refuse-downgrade --install "$deb"
        else
            printf '[PiSugar] Debian package %s is already installed and verified.\n' "$package_version" >&2
        fi
        step 'Verifying the installed Debian package version and architecture'
        record=$(deb_status "pisugar-programmer:$arch")
        IFS=$'\t' read -r status version architecture <<< "$record"
        if [[ "$status" != *' ok installed' || "$version" != "$package_version" || "$architecture" != "$arch" ]]; then
            echo 'dpkg did not confirm the requested programmer package and architecture.' >&2
            return 1
        fi
    elif [[ ! -x "$installed_tool" ]] || ! cmp -s "$tool_file" "$installed_tool"; then
        step "Updating the standalone programmer at $installed_tool"
        for visible in "$installed_tool" /usr/bin/pisugar-programmer; do
            [[ -x "$visible" ]] || continue
            version=$("$visible" --version)
            if [[ ! "$version" =~ ^pisugar-programmer[[:space:]]([0-9]+\.[0-9]+\.[0-9]+)$ ]]; then
                echo "Could not identify the installed version at $visible." >&2
                return 1
            fi
            version=${BASH_REMATCH[1]}
            if [[ "$(printf '%s\n' "$version" "$tool_version" | sort -V | tail -1)" != "$tool_version" ]]; then
                echo "Existing programmer $version is newer than $tool_version; stopping." >&2
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
    step 'Verifying the installed programmer and shell command'
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
    local mode=update target tool_sha binary_sha tool_bytes ota_url version action platform attempt decision detail prompt
    local reset_option=(-r)
    local arch='' deb='' tool_file installed_tool asset tool_version=2.4.5
    local package_version="$tool_version-1" expected_version="pisugar-programmer $tool_version"
    local release="https://cdn.pisugar.com/release/PiSugar3Firmware/programmer/v$tool_version"
    local ota_sha=f098d458b04e1294d3f78813d4686f10511ffe09e75cea1fb0349ff9a88d5c6d
    if (( $# == 1 )) && [[ "$1" == --check || "$1" == --install-only || "$1" == --wait-reset ]]; then
        mode=${1#--}
    elif (( $# )); then
        echo 'Usage: bash PiSugarUpdate.sh [--check | --install-only | --wait-reset]' >&2
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
    case "$mode" in
        check) operation='Download and firmware check' ;;
        install-only) operation='Programmer installation' ;;
        wait-reset) operation='PiSugar firmware update with manual Reset' ;;
        update) operation='PiSugar 3 firmware update' ;;
    esac
    finish_message="$operation complete."
    run_started=$SECONDS
    current_step='Starting updater'
    exec 8> >(watch_progress)
    reporter_pid=$!
    trap cleanup EXIT
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
    step "Starting $operation"
    work=$(mktemp -d /tmp/pisugar-update.XXXXXXXXXX)
    step 'Detecting the Raspberry Pi OS architecture'
    if command -v dpkg >/dev/null; then
        arch=$(dpkg --print-architecture)
        case "$arch" in
            arm64)
                tool_sha=a050b718b6c94bc4876606be45ea716d1e8f093ccb28af5bbb155aa57972e923
                binary_sha=02b31a8c102c952225004bdd159ef4d43e6f955567a978f48fd3e552b77cc5f6
                tool_bytes=1254522 ;;
            armhf)
                tool_sha=b7984b3e4e4235202d85e80cbb5f27915cae0317659ba84213af7a108d4156b4
                binary_sha=a56d69796e9f7c88dc19731582c7491279739f6058cb9d40d106ba1907214c98
                tool_bytes=1103800 ;;
            armel)
                tool_sha=f51477c296863f1bb3813e4734d25074548c5cc4d954d18479b82074506980cc
                binary_sha=e7c161df7a797bf56eb7d4f52ff3997d13cf8ef4f0e261f0f54c2baf791f3e52
                tool_bytes=1107582 ;;
            *) echo "Unsupported Debian architecture: $arch" >&2; return 1 ;;
        esac
        asset="pisugar-programmer_${package_version}_${arch}.deb"
        deb="$work/$asset"
        installed_tool=/usr/bin/pisugar-programmer
    else
        target=$(standalone_target)
        case "$target" in
            aarch64-unknown-linux-musl) tool_sha=02b31a8c102c952225004bdd159ef4d43e6f955567a978f48fd3e552b77cc5f6; tool_bytes=3725776 ;;
            armv7-unknown-linux-musleabihf) tool_sha=18f6d79581a900bd6abef425f37638d0d95b77a1db700cf5dcbf257c1a359cba; tool_bytes=3070260 ;;
            arm-unknown-linux-musleabi) tool_sha=e7c161df7a797bf56eb7d4f52ff3997d13cf8ef4f0e261f0f54c2baf791f3e52; tool_bytes=3126508 ;;
        esac
        binary_sha=$tool_sha
        installed_tool=/usr/local/bin/pisugar-programmer
    fi
    step "Checking for a local copy of $expected_version"
    if stage_installed_programmer; then
        tool_file="$work/local-programmer"
        step "Reusing verified $expected_version from $installed_tool (skipping programmer download)"
    elif [[ -n "$deb" ]]; then
        download "$release/$asset" "$deb" "$tool_sha" "$tool_bytes"
        step "Unpacking $asset"
        dpkg-deb -x "$deb" "$work/pkg"
        tool_file="$work/pkg$installed_tool"
    else
        tool_file="$work/pisugar-programmer"
        download "$release/pisugar-programmer-$tool_version-$target" "$tool_file" "$tool_sha" "$tool_bytes"
        chmod 700 "$tool_file"
    fi
    step 'Checking the programmer version'
    version=$("$tool_file" --version)
    if [[ "$version" != "$expected_version" ]]; then
        echo "Unexpected programmer version: $version" >&2
        return 1
    fi
    if [[ "$mode" != install-only ]]; then
        download "$ota_url" "$work/pisugar-3-ota.zip" "$ota_sha" 27807
        step 'Validating both firmware images in the OTA package'
        "$tool_file" --check "$work/pisugar-3-ota.zip"
    fi
    if [[ "$mode" == check ]]; then return 0; fi
    step 'Acquiring the PiSugar update lock'
    exec 9>>/run/pisugar-firmware-update.lock
    if ! flock -n 9; then
        echo 'Could not acquire the PiSugar firmware update lock.' >&2
        return 1
    fi
    install_programmer
    if [[ "$mode" == install-only ]]; then return 0; fi
    command -v python3 >/dev/null || { echo 'Python 3 is required to read the board version.' >&2; return 1; }
    prepare_i2c
    step 'Reading the PiSugar platform and installed firmware version over I2C'
    decision=$(plan_update wait-board)
    read -r action platform detail <<< "$decision"
    case "$platform" in fm33lc0x|ch32l103) ;; *) echo 'Unknown MCU; stopping.' >&2; return 1 ;; esac
    case "$action" in
        upgrade|recover) prompt='' ;;
        current) prompt="Reinstall the same firmware version on $platform?" ;;
        downgrade) prompt="Downgrade $platform from $detail to the older package version shown above?" ;;
        *) echo 'Unrecognized board version check result.' >&2; return 1 ;;
    esac
    if [[ -n "$prompt" ]]; then
        if (( restore_service )); then
            step 'Restarting pisugar-server while waiting for your decision'
            systemctl start pisugar-server
            restore_service=0
        fi
        confirm_flash "$prompt" || return 0
        prepare_i2c
        step 'Rechecking the MCU and installed version after confirmation'
        if ! action=$(plan_update) || [[ "$action" != "$decision" ]]; then
            echo 'The board state changed after confirmation. Run the update again.' >&2
            return 1
        fi
    fi
    # Bind the write to the platform we just checked. The programmer independently
    # reads the board identity before reset and rejects a BIN for the other MCU.
    local image="$work/pisugar-3-$platform-application.bin"
    step "Extracting the verified $platform application"
    python3 -c '
from pathlib import Path
import sys
from zipfile import ZipFile
with ZipFile(sys.argv[1]) as bundle:
    Path(sys.argv[3]).write_bytes(bundle.read("pisugar-3-" + sys.argv[2] + "-application.bin"))
' "$work/pisugar-3-ota.zip" "$platform" "$image"
    if [[ "$mode" == wait-reset || "$detail" == bootloader ]]; then
        reset_option=()
    fi
    echo 'Keep the Pi and PiSugar powered until the update finishes.'
    step "Flashing the $platform application (programmer reports accepted bytes)"
    "$installed_tool" "${reset_option[@]}" --yes "$image"
    for attempt in 1 2 3 4 5; do
        step "Checking the running $platform firmware after flashing (attempt $attempt/5)"
        if action=$(plan_update) && [[ "$action" == "current $platform" ]]; then
            echo "Installed firmware version verified for $platform."
            return 0
        fi
        if (( attempt < 5 )); then sleep 1; fi
    done
    echo 'The programmer finished, but the installed firmware version could not be confirmed.' >&2
    return 1
}

# Keep the call at EOF so piping a truncated function cannot start an update.
main "$@"
