#!/bin/bash
set -e
set -o pipefail

# version
version=2.4.0

# channel: nightly or release
channel=release

# arch: arm or arm64
arch=arm

# type: deb or rpm
type=deb

# rpm build number, default 1
rpm_n=1

# Auto mode keeps debconf interactive only when a terminal is available.
frontend=auto
model=""

# check distribution
if which apt >/dev/null 2>&1 && which dpkg >/dev/null 2>&1; then
    type=deb
elif which rpm >/dev/null 2>&1 ; then
    type=rpm
else
    echo "Unsupported linux distribution, dpkg/rpm not found"
    exit 1
fi

# check arch
if uname -m | grep aarch64; then
    arch=arm64
fi

prog="$0"

function usage() {
    cat << EOF
$prog

Install pisugar power manager tools.

USAGE: $prog [OPTIONS]

OPTIONS:
    -h|--help       Print this usage.
    -v|--version    Install a specified version, default: $version
    -c|--channel    Choose nightly or release channel, default: $channel
    -t|--type       Package type, deb or rpm, default: $type
    --rpm-build     RPM build number, default: $rpm_n
    -m|--model      PiSugar model (quote names containing spaces).
    --non-interactive  Install without debconf dialogs (automatic without a TTY).
    --interactive  Use debconf dialogs; requires a terminal.

Models: PiSugar 3, PiSugar 2 (2-LEDs), PiSugar 2 Pro, PiSugar 2 (4-LEDs).
Fresh unattended installs default to PiSugar 3; use --model for other hardware.
Existing debconf answers and authentication are preserved unless supplied again.

For more details, see https://github.com/PiSugar/pisugar-power-manager-rs

EOF
}

echo=$(which echo)
getopt=$(which getopt)
if ! ARGS=$($getopt -q -o hv:c:t:m: -l help,version:,channel:,type:,model:,rpm-build:,non-interactive,interactive -- "$@"); then
    usage && exit 1
fi

eval set -- "${ARGS}"

while true
do
    case "$1" in
        -h|--help)
            usage && exit 0
            ;;
        -v|--version)
            shift && version=$1
            ;;
        -c|--channel)
            shift && channel=$1
            ;;
        -t|--type)
            shift && type=$1
            ;;
        -m|--model)
            shift && model=$1
            ;;
        --non-interactive)
            frontend=noninteractive
            ;;
        --interactive)
            frontend=dialog
            ;;
        --rpm-build)
            shift && rpm_n=$1
            ;;
        --)
            shift && break
            ;;
        *)
            usage && exit 1
            ;;
    esac
    shift
done

# Validate before downloading or changing the system.
[ "$#" -eq 0 ] || { usage; exit 1; }
case "$type" in deb|rpm) ;; *) echo "Invalid package type: $type" >&2; exit 1 ;; esac
case "$channel" in release|nightly) ;; *) echo "Invalid channel: $channel" >&2; exit 1 ;; esac
case "$model" in
    ""|"PiSugar 3"|"PiSugar 2 (2-LEDs)"|"PiSugar 2 Pro"|"PiSugar 2 (4-LEDs)") ;;
    *) echo "Invalid PiSugar model: $model" >&2; exit 1 ;;
esac
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Invalid version" >&2; exit 1; }
[[ "$rpm_n" =~ ^[0-9]+$ ]] || { echo "Invalid RPM build number" >&2; exit 1; }
if [ "$frontend" = auto ]; then
    if [ -t 0 ] && [ -t 1 ] && [ "${DEBIAN_FRONTEND:-}" != noninteractive ]; then
        frontend=${DEBIAN_FRONTEND:-dialog}
    else
        frontend=noninteractive
    fi
elif [ "$frontend" = dialog ] && { [ ! -t 0 ] || [ ! -t 1 ]; }; then
    echo "--interactive requires a terminal; use --non-interactive --model MODEL" >&2
    exit 1
fi

# Calculate this after parsing --version.
deb_version=""
if [ "$(printf '%s\n' "$version" 1.7.6 | sort -V | head -n 1)" = 1.7.6 ]; then
    deb_version="-1"
fi

# package names
if [ "$type"x == "deb"x ]; then
    if dpkg_arch=$(dpkg --print-architecture); then
        arch_deb=${dpkg_arch}
    else
        if [ "$arch"x == "arm"x ]; then
            arch_deb=armhf
        else
            arch_deb=arm64
        fi
    fi
    package_server="pisugar-server_${version}${deb_version}_${arch_deb}.deb"
    package_poweroff="pisugar-poweroff_${version}${deb_version}_${arch_deb}.deb"
    package_programmer="pisugar-programmer_${version}${deb_version}_${arch_deb}.deb"
else
    if [ "$arch"x == "arm"x ]; then
        arch_rpm=armv7hl
    else
        arch_rpm=aarch64
    fi
    package_server="pisugar-server-${version}-${rpm_n}.${arch_rpm}.rpm"
    package_poweroff="pisugar-poweroff-${version}-${rpm_n}.${arch_rpm}.rpm"
    package_programmer="pisugar-programmer-${version}-${rpm_n}.${arch_rpm}.rpm"
fi

local_ip=$(hostname -I 2>/dev/null | awk '{print $1}') || true
local_ip=${local_ip:-localhost}

function install_pkgs() {
    if [ "$type" = deb ]; then
        if [ -n "$model" ]; then
            printf '%s\n' "pisugar-server pisugar-server/model select $model" \
                "pisugar-poweroff pisugar-poweroff/model select $model" | sudo debconf-set-selections
        fi
        sudo env DEBIAN_FRONTEND="$frontend" apt-get install -y jq debconf-utils
        echo "Debconf frontend: $frontend; fresh-install default model: PiSugar 3"
        # apt resolves dependencies and also configures previously unpacked packages.
        if ! sudo env DEBIAN_FRONTEND="$frontend" apt-get install -y "$@"; then
            echo "Installation failed. Fix the reported error, then rerun this installer." >&2
            echo "To retry configuration: sudo dpkg --configure pisugar-server pisugar-poweroff pisugar-programmer" >&2
            return 1
        fi
    else
        sudo rpm -Uvh "$@"
    fi
}

function enable_i2c() {
    if which raspi-config > /dev/null 2>&1; then
        sudo raspi-config nonint do_i2c 0
    else
        echo "raspi-config not found, please enable i2c manually!"
    fi
}

TEMPDIR=$(mktemp -d /tmp/pisugar.XXXXXX)
function cleanup() {
    rm -rf "$TEMPDIR"
}
trap cleanup EXIT

$echo -e "\033[1;34mDownload PiSugar-server and PiSugar-poweroff package \033[0m"
wget -O "$TEMPDIR/${package_server}" "https://cdn.pisugar.com/${channel}/${package_server}"
wget -O "$TEMPDIR/${package_poweroff}" "https://cdn.pisugar.com/${channel}/${package_poweroff}"
wget -O "$TEMPDIR/${package_programmer}" "https://cdn.pisugar.com/${channel}/${package_programmer}"


$echo -e "\033[1;34mOpen I2C Interface \033[0m"
enable_i2c

$echo -e "\033[1;34mInstall packages\033[0m"
install_pkgs "$TEMPDIR/${package_server}" "$TEMPDIR/${package_poweroff}" "$TEMPDIR/${package_programmer}"

$echo -e "\033[1;34mClean up \033[0m"
rm -f "$TEMPDIR/${package_server}"
rm -f "$TEMPDIR/${package_poweroff}"
rm -f "$TEMPDIR/${package_programmer}"

$echo -e "Now navigate to \033[1;34mhttp://${local_ip}:8421\033[0m on your browser to see PiSugar power management"
$echo -e "If you have any question, please feel free to contact us."
$echo -e "\033[1;34mThe PiSugar Team https://www.pisugar.com\033[0m"

cleanup
