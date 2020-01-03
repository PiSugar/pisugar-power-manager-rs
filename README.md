# pisugar-power-manager-rs

<p align="center">
  <img width="320" src="https://raw.githubusercontent.com/JdaieLin/PiSugar/master/logo.jpg">
</p>

## Management program for PiSugar 2

PiSugar power manager in rust language.

## Compilation

CPU architecture of raspberry pi is different from your linux/windows PC or macbook, there are two ways of compiling the code:

1. directly on raspberry pi
2. cross compilation

If you need more about rust cross compilation, please refer to https://dev.to/h_ajsf/cross-compiling-rust-for-raspberry-pi-4iai .

### On raspberry pi

Install rust

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    rustup update

Then

    cargo build --release

### Cross compilation - macos

Install cross compiler utils

    brew install arm-linux-gnueabihf-binutils

Install rust and armv6(zero/zerow) or armv7(3b/3b+) target

    brew install rustup-init
    rustup update
    rustup target add arm-unknown-linux-gnueabihf       # armv6
    rustup target add armv7-unknown-linux-gnueabihf     # armv7

Build

    RUSTC_LINKER=arm-linux-gnueabihf-ld cargo build --target arm-unknown-linux-gnueabihf --release

### Cross compilation - linux/ubuntu

Install cross compiler utils

    sudo apt-get install gcc-arm-linux-gnueabihf

Install rust and arm/armv7 target

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    rustup update
    rustup target add arm-unknown-linux-gnueabihf       # armv6
    rustup target add armv7-unknown-linux-gnueabihf     # armv7

Build

    RUSTC_LINKER=arm-linux-gnueabihf-ld cargo build --target arm-unknown-linux-gnueabihf --release

### Cross compilation - windows

Get cross toolchains from https://gnutoolchains.com/raspberry/

Install rust, please refer to https://forge.rust-lang.org/infra/other-installation-methods.html

### RLS

RLS configuration of vscode `.vscode/settings.json`

    {
        "rust.target": "arm-unknown-linux-gnueabihf"
    }

## LICENSE

GPL v3
