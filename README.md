# pisugar-power-manager-rs

![Master](https://github.com/PiSugar/pisugar-power-manager-rs/workflows/Master/badge.svg)
![Nightly](https://github.com/PiSugar/pisugar-power-manager-rs/workflows/Nightly%20build%20on%20master/badge.svg)

<p align="center">
  <img width="320" src="https://raw.githubusercontent.com/JdaieLin/PiSugar/master/logo.jpg">
</p>

## Management program for PiSugar 2/3

PiSugar power manager in rust language.

Python api: https://github.com/PiSugar/pisugar-server-py

## Install

Uninstall

    sudo dpkg --purge pisugar-server
    sudo dpkg --purge pisugar-poweroff

These packages are hosted in QiNiu CDN (For zero/zerowh/pi3/pi3b/pi4 with 32bit os, you might need to download and install packages manually in 64bit os)

    wget https://cdn.pisugar.com/release/pisugar-power-manager.sh
    bash pisugar-power-manager.sh -c release

Install script usage

    Install pisugar power manager tools.

    USAGE: pisugar-power-manager.sh [OPTIONS]

    OPTIONS:
        -h|--help       Print this usage.
        -v|--version    Install a specified version, default: 1.4.0
        -c|--channel    Choose nightly or release channel, default: release

    For more details, see https://github.com/PiSugar/pisugar-power-manager-rs

**NOTE** In centos/redhat like linux, RPM could not ask question in interactive mode, PiSugar model **MUST** be configured manually (`/etc/default/pisugar-*`). Available models are:

    PiSugar 2 (4-LEDs)
    PiSugar 2 (2-LEDs)
    PiSugar 2 Pro
    PiSugar 3

**NOTE** In pi-star, you need to add some iptables rules to allow access to web UI, see http://wiki.pistar.uk/Adding_custom_firewall_rules_to_Pi-Star

    echo 'iptables -A INPUT -p tcp --dport 8421 -j ACCEPT' | sudo tee -a /root/ipv4.fw
    echo 'iptables -A INPUT -p tcp --dport 8421 -j ACCEPT' | sudo tee -a /root/ipv6.fw
    sudo ipstar-firewall

Replace model in `/etc/default/pisugar-server`

    sed -e "s|--model '.*' |--model '<model>' |"
        -i /etc/default/pisugar-server

**NOTE** `auto_power_on` mode would prevent PiSugar falling into sleep, it could be useful in some cases. (since v1.4.8, `/etc/pisugar-server/config.json`)

Http authentication config in `/etc/pisugar/config.json` (replace `<username>` and `<password>`, default `admin/admin` )

    {
        ...
        digest_auth: ["<username>", "<password>"]
        ...
    }

## Install (ArchLinux only, unstable)

Download latest `pisugar-archlinux_<version>_all.tar.gz` from https://github.com/PiSugar/pisugar-power-manager-rs/releases

    tar -xvf pisugar-archlinux_<version>_all.tar.gz
    (cd pisugar-archlinux; sh install.sh)

## Linux kernel power supply driver

You might want to install the kernel driver to display battery status, see [pisugar-module/README.md](pisugar-module/README.md).

## Prerequisites

On raspberry pi, enable I2C interface

    sudo raspi-config

`Interfacing Options -> I2C -> Yes`

Known conflicts and issues:

    HyperPixel: HyperPixel disables I2C interface

## Modules

1. pisugar-core: Core library
2. pisugar-server: Http/tcp/uds server that provide PiSugar battery status
3. pisugar-poweroff: Systemd service that shut down PiSugar battery

## Non-interactive

Install `debconf-utils`

    sudo apt install -y debconf-utils

pisugar-server (REPLACE `<TOP SECRET>` WITH YOUR PASSWORD)

    sudo debconf-set-selections << EOF
    pisugar-server pisugar-server/model select PiSugar 3
    pisugar-server pisugar-server/auth-username string admin
    pisugar-server pisugar-server/auth-password password <TOP SECRET>
    EOF
    sudo DEBIAN_FRONTEND=noninteractive dpkg -i pisugar-server_<version>.deb

pisugar-poweroff

    sudo debconf-set-selections << EOF
    pisugar-poweroff pisugar-poweroff/model select PiSugar 3
    EOF
    sudo DEBIAN_FRONTEND=noninteractive dpkg -i pisugar-poweroff_<version>.deb

See `debian/templates` in each child project directory.

## Configuration

Now, navigate to `http://x.x.x.x:8421` on your browser and see PiSugar power status.

Configuration files of pisugar-server

    /etc/default/pisugar-server
    /etc/pisugar-server/config.json

Configuration files of pisugar-poweroff

    /etc/default/pisugar-poweroff

## RLS

RLS configuration of vscode `.vscode/settings.json`

    {
        "rust.target": "arm-unknown-linux-musleabihf"
    }

### Unix domain socket/ websocket / tcp API

Default ports:

    uds     /tmp/pisugar-server.sock
    tcp     0.0.0.0:8423
    ws      0.0.0.0:8422    # standalone websocket api
    http    0.0.0.0:8421    # web UI and websocket (/ws)

To get the full command list, please send a `help xx` request.

| Command | Description | Response/Usage |
| :- | :-: | :-: |
| get firmware_version    | firmware version | firmware_version: [string] |
| get battery             | battery level % | battery: [number] |
| get battery_i           | BAT current in A (PiSugar 2 only) | battery_i: [number] |
| get battery_v           | BAT voltage in V | battery_v: [number] |
| get battery_charging    | charging status (for new model please use battery_power_plugged and battery_allow_charging to get charging status)  | battery_charging: [true\|false] |
| get battery_input_protect_enabled  | BAT input protect enabled | battery_input_protect_enable: [true\|false] |
| get model               | pisugar model | model: PiSugar 2 |
| get battery_keep_input  | Keep power input when reading voltage | battery_keep_input: [true\|false] |
| get battery_led_amount  | charging led amount (2 is for new model) | battery_led_amount: [2\|4] |
| get battery_power_plugged  | charging usb plugged (new model only) | battery_power_plugged: [true\|false] |
| get battery_charging_range | charging range restart_point% stop_point% (new model only)  | battery_charging_range: [number, number]|
| get battery_allow_charging | whether charging is allowed when usb is plugged  (new model only)  | battery_allow_charging: [true\|false]|
| get battery_output_enabled | battery output status | battery_output_enabled: [true\|false] |
| get rtc_time            | rtc clock | rtc_time: [ISO8601 time string] |
| get rtc_alarm_enabled   | rtc wakeup alarm enable | rtc_alarm_enabled: [true\|false] |
| get rtc_alarm_time      | rtc wakeup alarm time | rtc_alarm_time: [ISO8601 time string] |
| get alarm_repeat        | rtc wakeup alarm repeat in weekdays (127=1111111) | alarm_repeat: [number] |
| get button_enable       | custom button enable status | button_enable: [single\|double\|long] [true\|false] |
| get button_shell        | shell script when button is clicked  | button_shell: [single\|double\|long] [shell] |
| get safe_shutdown_level | auto shutdown level | safe_shutdown_level: [number] |
| get safe_shutdown_delay | auto shutdown delay | safe_shutdown_delay: [number] |
| get rtc_adjust_ppm | (pisugar3) adjust rtc ppm | rtc_adjust_ppm: [number] |
| get auth_username | http auth username  | auth_username: [string] |
| get anti_mistouch | anti-mistouch | anti_mistouch: [true\|false] |
| get soft_poweroff | software poweroff | soft_poweroff: [true\|false] |
| get soft_poweroff_shell | soft poweroff shell script | soft_poweroff_shell: [string] |
| get temperature | chip temperature | temperature: [number] |
| get input_protect | battery hardware protect | input_protect: [true\|false] |
| rtc_pi2rtc | sync time pi => rtc | |
| rtc_rtc2pi | sync time rtc => pi | |
| rtc_web | sync time web => rtc & pi | |
| rtc_alarm_set | set rtc wakeup alarm | rtc_alarm_set [ISO8601 time string] [repeat] |
| rtc_alarm_disable | disable rtc wakeup alarm | rtc_alarm_disable |
| rtc_adjust_ppm | (pisugar3) adjust rtc ppm, -500.0 to 500.0 | rtc_adjust_ppm [number] |
| set_battery_keep_input | Set keep power input when reading voltage | set_battery_keep_input [true\|false] |
| set_button_enable | auto shutdown level % | set_button_enable [single\|double\|long] [0\|1] |
| set_button_shell | auto shutdown level | safe_shutdown_level [single\|double\|long] [shell] |
| set_battery_input_protect | set BAT input protect | set_battery_input_protect [true\|false] |
| set_safe_shutdown_level | set auto shutdown level % | safe_shutdown_level [number] |
| set_safe_shutdown_delay | set auto shutdown delay in second | safe_shutdown_delay [number]|
| set_battery_charging_range | set charging range | set_battery_charging_range [number, number]|
| set_allow_charging | enable or disable charging | set_allow_charging [true\|false] |
| set_battery_output | enable or disable battery output | set_battery_output [true\|false] |
| set_auth | set or clear http auth (with no arguments) | set_auth [username password] |
| set_anti_mistouch | enable or disable anti-mistouch | set_anti_mistouch [true\|false] |
| set_soft_poweroff | enable or disable software poweroff | set_soft_poweroff [true\|false] |
| set_soft_poweroff_shell | soft poweroff shell | set_soft_poweroff_shell [string] |
| set_input_protect | enable or disable battery hardware protect | set_input_protect [true\|false] |

Examples:

    nc -U /tmp/pisugar-server.sock
    get battery
    get model
    rtc_alarm_set 2020-06-26T16:09:34+08:00 127
    set_button_enable long 1
    set_button_enable long sudo shutdown now
    safe_shutdown_level 3
    safe_shutdown_delay 30
    <ctrl+c to break>

Or

    echo "get battery" | nc -q 0 127.0.0.1 8423

## Release

See https://github.com/PiSugar/pisugar-power-manager-rs/releases

## LICENSE

GPL v3
