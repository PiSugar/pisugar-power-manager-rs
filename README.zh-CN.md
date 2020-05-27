# pisugar-power-manager-rs

![Latest master](https://github.com/PiSugar/pisugar-power-manager-rs/workflows/Nightly%20build%20on%20master/badge.svg?branch=master)
![Latest PR](https://github.com/PiSugar/pisugar-power-manager-rs/workflows/PR%20build%20on%20master/badge.svg)

<p align="center">
  <img width="320" src="https://raw.githubusercontent.com/JdaieLin/PiSugar/master/logo.jpg">
</p>

## PiSugar 2 电池管理程序

采用 Rust 编写的 PiSugar 2 电池管理程序。

## 安装

安装包托管在七牛云。

    curl http://cdn.pisugar.com/release/Pisugar-power-manager.sh | sudo sh

或

    wget http://cdn.pisugar.com/release/Pisugar-power-manager.sh
    bash Pisugar-power-manager.sh -c release

安装脚本使用方式：

    Install pisugar power manager tools.

    USAGE: Pisugar-power-manager.sh [OPTIONS]

    OPTIONS:
        -h|--help       Print this usage.
        -v|--version    Install a specified version, default: 1.4.0
        -c|--channel    Choose nightly or release channel, default: release

    For more details, see https://github.com/PiSugar/pisugar-power-manager-rs

## 开启 I2C 功能

树莓派上

    sudo raspi-config

`Interfacing Options -> I2C -> Yes`

## 模块划分

1. pisugar-core: 核心库
2. pisugar-server: Http/tcp/uds 服务器
3. pisugar-poweroff: Systemd 关机服务

## 编译

树莓派的 CPU 指令集与 linux/windows pc 或 macbook 的不同，有两种方式进行编译：

1. 树莓派上编译
2. 交叉编译

注意：如果有 cargo 报告网络问题，移除 .cargo/config 的 `replace-with=...` 配置项。

### 树莓派上编译

安装 Rust

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    rustup update

编译

    cargo build --release

### 交叉编译 - macos (musl)

安装交叉编译工具

    brew install FiloSottile/musl-cross/musl-cross --without-x86_64 --with-arm-hf

安装 Rust armv6(zero/zerow) 或 armv7(3b/3b+) 目标工具

    brew install rustup-init
    rustup update
    rustup target add arm-unknown-linux-musleabihf      # armv6
    rustup target add armv7-unknown-linux-musleabihf    # armv7

编译

    cargo build --target arm-unknown-linux-musleabihf --release     # armv6
    cargo build --target armv7-unknown-linux-musleabihf --release   # armv7

### 交叉编译 - linux/ubuntu (musl)

安装交叉编译工具 (预编译的 musl x86_64 或 i686 工具链)

    wget https://more.musl.cc/$(uname -m)-linux-musl/arm-linux-musleabihf-cross.tgz
    tar -xvf arm-linux-musleabihf-cross.tgz

放到 `/opt`, 并加入 `PATH`

    sudo mv arm-linux-musleabihf-cross /opt/
    echo 'export PATH=/opt/arm-linux-musleabihf-cross/bin:$PATH' >> ~/.bashrc
    source ~/.bashrc

安装 Rust arm/armv7 目标工具

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    rustup update
    rustup target add arm-unknown-linux-musleabihf       # armv6
    rustup target add armv7-unknown-linux-musleabihf     # armv7

编译

    cargo build --target arm-unknown-linux-musleabihf --release      # armv6
    cargo build --target armv7-unknown-linux-musleabihf --release    # armv7

### 交叉编译 - windows

使用 WSL 并按照 linux 的交叉编译设置。

### 编译和安装 deb 包

编译前端 web

    (cd electron && npm install && npm run build:web)

Election 下载失败时，尝试镜像服务器

    ELECTRON_MIRROR="https://npm.taobao.org/mirrors/electron/" npm install

使用 cargo-deb (需要最新的 cargo-deb)

    cargo install --git https://github.com/mmstick/cargo-deb.git

    # linux
    PATH="$(pwd)/arm-linux-musleabihf-cross/bin:$PATH" \
        cargo deb --target arm-unknown-linux-musleabihf --manifest-path=pisugar-server/Cargo.toml

    PATH="$(pwd)/arm-linux-musleabihf-cross/bin:$PATH" \
    cargo deb --target arm-unknown-linux-musleabihf --manifest-path=pisugar-poweroff/Cargo.toml

    # macos
    cargo deb --target arm-unknown-linux-musleabihf --manifest-path=pisugar-server/Cargo.toml
    cargo deb --target arm-unknown-linux-musleabihf --manifest-path=pisugar-poweroff/Cargo.toml

安装

    # Install
    sudo dpkg -i pisugar-xxx_<version>_<arch>.deb

    # Uninstall/Purge
    sudo dpkg -P pisugar-xxx

控制 pisugar-server systemd 服务的命令

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

 (pisugar-poweroff 在系统 poweroff 前执行一次，关闭芯片)

在浏览器访问 `http://x.x.x.x:8421`，并查看 PiSugar 2 电池状态。

pisugar-server 的配置文件

    /etc/default/pisugar-server
    /etc/pisugar-server/config.json

pisugar-poweroff 的配置文件

    /etc/default/pisugar-poweroff

如果要在安装后重新配置

    sudo dpkg-reconfigure pisugar-server
    sudo dpkg-reconfigure pisugar-poweroff

在安装前进行配置

    sudo dpkg-preconfigure pisugar-server_<ver>_<arch>.deb
    sudo dpkg-preconfigure pisugar-poweroff_<ver>_<arch>.deb

### vscode RLS 配置

vscode RLS 配置 `.vscode/settings.json`

    {
        "rust.target": "arm-unknown-linux-musleabihf"
    }

### Unix Domain Socket / Webscoket / TCP

默认端口列表:

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
| get safe_shutdown_delay | auto shutdown delay | safe_shutdown_delay: [number] |
| rtc_pi2rtc | sync time pi => rtc | |
| rtc_rtc2pi | sync time rtc => pi | |
| rtc_web | sync time web => rtc & pi | |
| rtc_alarm_set | set rtc wakeup alarm | rtc_alarm_set: [ISO8601 time string] [repeat] |
| rtc_alarm_disable | disable rtc wakeup alarm | |
| set_button_enable | auto shutdown level % | set_button_enable: [single\|double\|long] [0\|1] |
| set_button_shell | auto shutdown level | safe_shutdown_level: [single\|double\|long] [shell] |
| set_safe_shutdown_level | set auto shutdown level % | safe_shutdown_level: 3 |
| set_safe_shutdown_delay | set auto shutdown delay in second | safe_shutdown_delay: 30|

示例：

    nc -U /tmp/pisugar-server.sock
    get battery
    get model
    <ctrl+c to break>

或

    echo "get battery" | nc -q 0 127.0.0.1 8423

## 版本发布

查看 https://github.com/PiSugar/pisugar-power-manager-rs/releases

## LICENSE

GPL v3
