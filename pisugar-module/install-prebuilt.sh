#!/usr/bin/env bash
set -euo pipefail

MODULE_NAME=pisugar_battery
KERNEL_RELEASE=${KERNEL_RELEASE:-$(uname -r)}
SOURCE=${1:-pisugar_battery.ko}
DESTINATION="/lib/modules/${KERNEL_RELEASE}/kernel/drivers/power/supply/${MODULE_NAME}.ko"
I2C_BUS=${I2C_BUS:-1}
PISUGAR3_ADDR=${PISUGAR3_ADDR:-0x57}

read_server_i2c_config() {
    local config=/etc/pisugar-server/config.json
    local value

    if ! sudo test -f "$config"; then
        return 0
    fi

    value=$(sudo sed -nE 's/^[[:space:]]*"i2c_bus"[[:space:]]*:[[:space:]]*([0-9]+).*/\1/p' "$config" | head -n 1)
    if [[ -n "$value" ]]; then
        I2C_BUS=$value
    fi

    value=$(sudo sed -nE 's/^[[:space:]]*"i2c_addr"[[:space:]]*:[[:space:]]*(0[xX][0-9a-fA-F]+|[0-9]+).*/\1/p' "$config" | head -n 1)
    if [[ -n "$value" ]]; then
        PISUGAR3_ADDR=$value
    fi
}

if [[ ! -f "$SOURCE" ]]; then
    echo "PiSugar module not found: $SOURCE" >&2
    exit 1
fi

read_server_i2c_config

sudo modprobe -r pisugar_3_battery 2>/dev/null || true
sudo modprobe -r pisugar_2_battery 2>/dev/null || true
sudo modprobe -r "$MODULE_NAME" 2>/dev/null || true
sudo sed -i '/^pisugar_[23]_battery$/d' /etc/modules 2>/dev/null || true
sudo rm -f /etc/modprobe.d/pisugar_2_battery.conf /etc/modprobe.d/pisugar_3_battery.conf
sudo install -D -m 644 "$SOURCE" "$DESTINATION"
echo "$MODULE_NAME" | sudo tee /etc/modules-load.d/pisugar-battery.conf >/dev/null
printf 'options %s i2c_bus=%s pisugar3_addr=%s\n' "$MODULE_NAME" "$I2C_BUS" "$PISUGAR3_ADDR" |
    sudo tee /etc/modprobe.d/pisugar-battery.conf >/dev/null
sudo depmod -a "$KERNEL_RELEASE"
sudo modprobe "$MODULE_NAME"

echo "Installed ${MODULE_NAME} for ${KERNEL_RELEASE}."
