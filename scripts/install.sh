#!/usr/bin/env bash

set -e
set -o pipefail


function print_usage() {
    echo "Usage: $0 <-u> <-m> [all|server|poweroff|programmer|module]"
    echo "Options:"
    echo "  -u         Uninstall the specified component(s) instead of installing"
    echo "  -m MODEL   Specify the PiSugar model (default: PiSugar 3)"
    echo "             Available models: PiSugar 3, PiSugar 2 (2-LEDs), PiSugar 2 Pro, PiSugar 2 (4-LEDs)"
    echo "  all        Install all components"
    echo "  server     Install PiSugar Server"
    echo "  poweroff   Install PiSugar Poweroff"
    echo "  programmer Install PiSugar Programmer"
    echo "  module     Install the prebuilt desktop battery kernel module"
}

function install_pisugar_server() {
    echo "Installing PiSugar Server..."
    sudo install -D -m 755 pisugar-server /usr/bin/pisugar-server
    sudo install -D -m 644 pisugar-server-conf/config.json /etc/pisugar-server/config.json
    sudo install -D -m 644 pisugar-server-conf/pisugar-server.service /lib/systemd/system/pisugar-server.service
    sudo install -D -m 644 pisugar-server-conf/pisugar-server.default /etc/default/pisugar-server
    for i in $(find web-ui -type f); do
        sudo install -D -m 644 $i /usr/share/pisugar-server/web/${i#web-ui/}
    done
    model=$1
    sudo sed -e "s|'PiSugar 2 (2-LEDs)'|'${model}'|g" -i /etc/default/pisugar-server
    sudo systemctl daemon-reload
    echo "PiSugar Server installed, please update the settings and run systemctl enable pisugar-server.service and systemctl start pisugar-server.service."
}

function uninstall_pisugar_server() {
    echo "Uninstalling PiSugar Server..."
    sudo systemctl stop pisugar-server.service || true
    sudo systemctl disable pisugar-server.service || true
    sudo rm -f /usr/bin/pisugar-server
    sudo rm -rf /etc/pisugar-server
    sudo rm -f /lib/systemd/system/pisugar-server.service
    sudo rm -f /etc/default/pisugar-server
    sudo rm -rf /usr/share/pisugar-server/web
    sudo systemctl daemon-reload
    echo "PiSugar Server uninstalled."
}

function install_pisugar_poweroff() {
    echo "Installing PiSugar Poweroff..."
    sudo install -D -m 755 pisugar-poweroff /usr/bin/pisugar-poweroff
    sudo install -D -m 644 pisugar-poweroff-conf/pisugar-poweroff.service /lib/systemd/system/pisugar-poweroff.service
    sudo install -D -m 644 pisugar-poweroff-conf/pisugar-poweroff.default /etc/default/pisugar-poweroff
    model=$1
    sudo sed -e "s|'PiSugar 3'|'${model}'|g" -i /etc/default/pisugar-poweroff
    sudo systemctl daemon-reload
    echo "PiSugar Poweroff installed, please update the settings and run systemctl enable pisugar-poweroff.service."
}

function uninstall_pisugar_poweroff() {
    echo "Uninstalling PiSugar Poweroff..."
    sudo systemctl stop pisugar-poweroff.service || true
    sudo systemctl disable pisugar-poweroff.service || true
    sudo rm -f /usr/bin/pisugar-poweroff
    sudo rm -f /lib/systemd/system/pisugar-poweroff.service
    sudo rm -f /etc/default/pisugar-poweroff
    sudo systemctl daemon-reload
    echo "PiSugar Poweroff uninstalled."
}

function install_pisugar_programmer() {
    echo "Installing PiSugar Programmer..."
    sudo install -D -m 755 pisugar-programmer /usr/bin/pisugar-programmer
    echo "PiSugar Programmer installed."
}

function uninstall_pisugar_programmer() {
    echo "Uninstalling PiSugar Programmer..."
    sudo rm -f /usr/bin/pisugar-programmer
    echo "PiSugar Programmer uninstalled."
}

function module_architecture() {
    case "$(uname -m)" in
        aarch64|arm64) echo arm64 ;;
        armv6l|armv7l|armhf) echo armhf ;;
        *) return 1 ;;
    esac
}

function is_raspberry_pi_system() {
    if [ -r /proc/device-tree/model ] && tr -d '\0' < /proc/device-tree/model | grep -q 'Raspberry Pi'; then
        return 0
    fi
    case "$(uname -r)" in
        *+rpt-rpi-*) return 0 ;;
    esac
    return 1
}

function install_pisugar_module() {
    local kernel_release module_arch package base_url temp_dir
    if ! is_raspberry_pi_system; then
        echo "Skip PiSugar desktop battery module: this is not a Raspberry Pi system"
        return 0
    fi
    kernel_release=$(uname -r)
    if ! module_arch=$(module_architecture); then
        echo "Skip PiSugar desktop battery module: unsupported architecture $(uname -m)"
        return 0
    fi
    package="pisugar-module_${kernel_release}_${module_arch}.tar.gz"
    base_url=${PISUGAR_MODULE_BASE_URL:-https://cdn.pisugar.com/release}
    temp_dir=$(mktemp -d /tmp/pisugar-module.XXXXXX)
    if ! curl -fL "$base_url/$package" -o "$temp_dir/$package"; then
        echo "No prebuilt module for kernel ${kernel_release} (${module_arch})." >&2
        echo "Update Raspberry Pi OS to a supported kernel and try again." >&2
        rm -rf "$temp_dir"
        return 0
    fi
    tar -xzf "$temp_dir/$package" -C "$temp_dir"
    bash "$temp_dir/install.sh" "$temp_dir/pisugar_battery.ko"
    rm -rf "$temp_dir"
}

function uninstall_pisugar_module() {
    sudo modprobe -r pisugar_battery 2>/dev/null || true
    sudo rm -f "/lib/modules/$(uname -r)/kernel/drivers/power/supply/pisugar_battery.ko"
    sudo rm -f /etc/modules-load.d/pisugar-battery.conf
    sudo rm -f /etc/modprobe.d/pisugar-battery.conf
    sudo depmod -a
    echo "PiSugar desktop battery module uninstalled."
}

if [ $# -lt 1 ]; then
    print_usage
    exit 1
fi

UNINSTALL=0
APP="all"
MODEL="PiSugar 3"
ARGS=$(getopt -q -o hum: --name "$0" -- "$@")
if [ $? != 0 ]; then
    print_usage && exit 1
fi

eval set -- "${ARGS}"
while true
do
    case "$1" in
        -h)
            print_usage && exit 0
            ;;
        -u)
            UNINSTALL=1
            ;;
        -m)
            shift && MODEL=$1
            ;;
        --)
            shift && APP=$1 && break
            ;;
        *)
            print_usage && exit 1
            ;;
    esac
    shift
done

# Verify MODEL
if [[ "$MODEL" != "PiSugar 3" && "$MODEL" != "PiSugar 2 (2-LEDs)" && "$MODEL" != "PiSugar 2 Pro" && "$MODEL" != "PiSugar 2 (4-LEDs)" ]]; then
    echo "Error: Unsupported model '$MODEL'."
    print_usage && exit 1
fi

if [ $UNINSTALL -eq 1 ]; then
    echo "Uninstalling $APP..."
    case "$APP" in
        all)
            uninstall_pisugar_server && \
            uninstall_pisugar_poweroff && \
            uninstall_pisugar_programmer && \
            uninstall_pisugar_module
            ;;
        server|poweroff|programmer|module)
            uninstall_pisugar_$APP
            ;;
        *)
            print_usage && exit 1
            ;;
    esac
else
    echo "Installing $APP, model ${MODEL}..."
    case "$APP" in
        all)
            install_pisugar_server "$MODEL" && \
            install_pisugar_poweroff "$MODEL" && \
            install_pisugar_programmer "$MODEL" && \
            install_pisugar_module
            ;;
        server|poweroff|programmer|module)
            install_pisugar_$APP "$MODEL"
            ;;
        *)
            print_usage && exit 1
            ;;
    esac
fi
