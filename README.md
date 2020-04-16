# pisugar-power-manager-rs

![Latest master](https://github.com/PiSugar/pisugar-power-manager-rs/workflows/Hourly%20build%20on%20master/badge.svg?branch=master)
![Latest PR](https://github.com/PiSugar/pisugar-power-manager-rs/workflows/PR%20build%20on%20master/badge.svg)

<p align="center">
  <img width="320" src="https://raw.githubusercontent.com/JdaieLin/PiSugar/master/logo.jpg">
</p>

## Management program for PiSugar 2

PiSugar power manager in rust language.

## Enable I2C interface

On raspberry pi

    sudo raspi-config

`Interfacing Options -> I2C -> Yes`

## Modules

1. pisugar-core: Core libary
2. pisugar-server: Http/tcp/uds server that provide PiSugar battery status
3. pisugar-poweroff: Systemd service that shut down PiSugar battery

## Compilation

CPU architecture of raspberry pi is different from your linux/windows PC or macbook, there are two ways of compiling the code:

1. directly on raspberry pi
2. cross compilation

If you need more about rust cross compilation, please refer to https://dev.to/h_ajsf/cross-compiling-rust-for-raspberry-pi-4iai .

### On raspberry pi

Install rust

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    rustup update

Build

    cargo build --release

### Cross compilation - macos

Install cross compiler utils

    brew install FiloSottile/musl-cross/musl-cross --without-x86_64 --with-arm-hf

Install rust and armv6(zero/zerow) or armv7(3b/3b+) target

    brew install rustup-init
    rustup update
    rustup target add arm-unknown-linux-musleabihf      # armv6
    rustup target add armv7-unknown-linux-musleabihf    # armv7

Build

    cargo build --target arm-unknown-linux-musleabihf --release     # armv6
    cargo build --target armv7-unknown-linux-musleabihf --release   # armv7

### Cross compilation - linux/ubuntu

Install cross compiler utils

    sudo apt-get install gcc-arm-linux-gnueabihf

Install rust and arm/armv7 target

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    rustup update
    rustup target add arm-unknown-linux-gnueabihf       # armv6
    rustup target add armv7-unknown-linux-gnueabihf     # armv7

Build

    cargo build --target arm-unknown-linux-gnueabihf --release      # armv6
    cargo build --target armv7-unknown-linux-gnueabihf --release    # armv7

### Cross compilation - windows

Get cross toolchains from https://gnutoolchains.com/raspberry/

Install rust, please refer to https://forge.rust-lang.org/infra/other-installation-methods.html

You might install WSL and follow the linux cross compilation steps.

### Build and install deb package

Build web content

    (cd electron && npm install && npm run build:web)

Build deb with cargo-deb (need latest cargo-deb that support templates)

    cargo install --git https://github.com/mmstick/cargo-deb.git

    # linux
    cargo deb --target arm-unknown-linux-gnueabihf --manifest-path=pisugar-server/Cargo.toml
    cargo deb --target arm-unknown-linux-gnueabihf --manifest-path=pisugar-poweroff/Cargo.toml

    # macos
    cargo deb --target arm-unknown-linux-musleabihf --manifest-path=pisugar-server/Cargo.toml
    cargo deb --target arm-unknown-linux-musleabihf --manifest-path=pisugar-poweroff/Cargo.toml

Install

    # Install
    sudo dpkg -i pisugar-xxx_<version>_<arch>.deb

    # Uninstall/Purge
    sudo dpkg -P pisugar-xxx

Commands of controlling pisugar-server systemd service

    # reload daemon
    sudo systemctl daemon-reload

    # check status
    sudo systemctl status pisugar-server

    # start service
    sudo systemctl start pisugar-server

    # stop service
    sudo systemctl stop pisugar-server

    # disable service
    sudo systemctl disable pisugar-server

    # enable service
    sudo systemctl enable pisugar-server

 (pisugar-poweroff run once just before linux poweroff)

Now, navigate to `http://x.x.x.x:8421` on your browser and see PiSugar power status.

Configuration files of pisugar-server

    /etc/default/pisugar-server
    /etc/pisugar-server/config.json

Configuration files of pisugar-poweroff

    /etc/default/pisugar-poweroff

To reconfigure after installation

    sudo dpkg-reconfigure pisugar-server
    sudo dpkg-reconfigure pisugar-poweroff

To preconfigure before installation

    sudo dpkg-preconfigure pisugar-server_<ver>_<arch>.deb
    sudo dpkg-preconfigure pisugar-poweroff_<ver>_<arch>.deb

### RLS

RLS configuration of vscode `.vscode/settings.json`

    {
        "rust.target": "arm-unknown-linux-gnueabihf"
    }

### Unix Domain Socket / Webscoket / TCP

Default ports:

    uds     /tmp/pisugar-server.sock
    tcp     0.0.0.0:8423
    ws      0.0.0.0:8422
    http    0.0.0.0:8421    # web only

| Command | Description | Response/Usage |
| :- | :-: | :-: |
| get battery             | battery level % | battery: [number] |
| get battery_i           | BAT current in A | battery_i: [number] |
| get battery_v           | BAT votage in V | battery_v: [number] |
| get battery_charging    | charging status  | battery_charging: [true\|false] |
| get model               | pisugar model | model: PiSugar 2 |
| get rtc_time            | rtc clock | rtc_time: [ISO8601 time string] |
| get rtc_alarm_enabled   | rtc wakeup alarm enable | rtc_alarm_enabled: [true\|false] |
| get rtc_alarm_time      | rtc wakeup alarm time | rtc_alarm_time: [ISO8601 time string] |
| get alarm_repeat        | rtc wakeup alarm repeat in weekdays (127=1111111) | alarm_repeat: [number] |
| get button_enable       | custom button enable status | button_enable: [single\|double\|long] [true\|false] |
| get button_shell        | shell script when button is clicked  | button_shell: [single\|double\|long] [shell] |
| get safe_shutdown_level | auto shutdown level | safe_shutdown_level: [number] |
| rtc_pi2rtc | sync time pi => rtc | |
| rtc_rtc2pi | sync time rtc => pi | |
| rtc_web | sync time web => rtc & pi | |
| rtc_alarm_set | set rtc wakeup alarm | rtc_alarm_set: [ISO8601 time string] [repeat] |
| rtc_alarm_disable | disable rtc wakeup alarm | |
| set_button_enable | auto shutdown level % | set_button_enable: [single\|double\|long] [0\|1] |
| set_button_shell | auto shutdown level | safe_shutdown_level: [single\|double\|long] [shell] |
| set_safe_shutdown_level | set auto shutdown level % | safe_shutdown_level: 3 |

Examples:

    nc -U /tmp/pisugar-server.sock
    get battery
    get model
    <ctrl+c to break>

## Release

See https://github.com/PiSugar/pisugar-power-manager-rs/releases

## LICENSE

GPL v3
