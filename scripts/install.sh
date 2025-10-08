#!/usr/bin/env bash

set -e
set -o pipefail


function print_usage() {
    echo "Usage: $0 <-u> [all|server|poweroff|programmer]"
    echo "Options:"
    echo "  -u         Uninstall the specified component(s) instead of installing"
    echo "  all        Install all components"
    echo "  server     Install PiSugar Server"
    echo "  poweroff   Install PiSugar Poweroff"
    echo "  programmer Install PiSugar Programmer"
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
    sudo install -D -m 755 target/release/pisugar-programmer /usr/bin/pisugar-programmer
    echo "PiSugar Programmer installed."
}

function uninstall_pisugar_programmer() {
    echo "Uninstalling PiSugar Programmer..."
    sudo rm -f /usr/bin/pisugar-programmer
    echo "PiSugar Programmer uninstalled."
}

if [ $# -lt 1 ]; then
    print_usage
    exit 1
fi

UNINSTALL=0
APP="all"
ARGS=$(getopt -q -o hu --name "$0" -- "$@")
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
        --)
            shift && APP=$1 && break
            ;;
        *)
            print_usage && exit 1
            ;;
    esac
    shift
done

if [ $UNINSTALL -eq 1 ]; then
    echo "Uninstalling $APP..."
    case "$APP" in
        all)
            uninstall_pisugar_server && \
            uninstall_pisugar_poweroff && \
            uninstall_pisugar_programmer
            ;;
        server|poweroff|programmer)
            uninstall_pisugar_$APP
            ;;
        *)
            print_usage && exit 1
            ;;
    esac
else
    echo "Installing $APP..."
    case "$APP" in
        all)
            install_pisugar_server && \
            install_pisugar_poweroff && \
            install_pisugar_programmer
            ;;
        server|poweroff|programmer)
            install_pisugar_$APP
            ;;
        *)
            print_usage && exit 1
            ;;
    esac
fi
