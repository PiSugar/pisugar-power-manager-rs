# Build from scratch

CPU architecture of raspberry pi is different from your linux/windows PC or macbook, there are two ways of compiling the code:

1. directly on raspberry pi
2. cross compilation

**NOTE** Remove `replace-with=...` in .cargo/config if cargo reports `warning: spurious network error`.

**NOTE** Need a static link with libgcc when cross compiling for Pi4 with aarch64

    # linux
    LIBGCC=$(find /opt/aarch64-linux-musl-cross -name libgcc.a)
    sed -e "s|\"/opt/aarch64-linux-musl-cross/lib/gcc/aarch64-linux-musl/9.2.1\"|\"${LIBGCC%/*}\"|" -i .cargo/config

    # macos
    LIBGCC=$(find find /usr/local/Cellar/musl-cross -name libgcc.a | grep aarch64)
    sed -e "s|\"/opt/aarch64-linux-musl-cross/lib/gcc/aarch64-linux-musl/9.2.1\"|\"${LIBGCC%/*}\"|" -i .cargo/config

## Raspberry pi

Install rust

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    rustup update

Build

    cargo build --release

## Cross compilation - macos (musl)

Install cross compiler utils

    # x86
    brew install FiloSottile/musl-cross/musl-cross --without-x86_64 --with-arm-hf   # arm
    brew install FiloSottile/musl-cross/musl-cross --without-x86_64 --with-aarch64  # arm64
    
    # m1
    brew tap richard-vd/musl-cross/musl-cross
    brew install richard-vd/musl-cross/musl-cross --without-x86_64 --with-arm-hf
    brew install richard-vd/musl-cross/musl-cross --without-x86_64 --with-aarch64


Install rust and armv6(zero/zerow) / armv7(3b/3b+) / arm64(i.e. aarch64, 4) target

    brew install rustup-init
    rustup update
    rustup target add arm-unknown-linux-musleabihf      # armv6
    rustup target add armv7-unknown-linux-musleabihf    # armv7
    rustup target add aarch64-unknown-linux-musl        # arm64

Build

    cargo build --target arm-unknown-linux-musleabihf --release     # armv6
    cargo build --target armv7-unknown-linux-musleabihf --release   # armv7
    cargo build --target aarch64-unknown-linux-musl                 # arm64

## Cross compilation - linux/ubuntu (musl)

Install cross compiler utils (prebuilt musl toolchain on x86_64 or i686)

    wget https://more.musl.cc/$(uname -m)-linux-musl/arm-linux-musleabihf-cross.tgz
    tar -xvf arm-linux-musleabihf-cross.tgz

Move the toolchain into `/opt`, and add it into `PATH`

    sudo mv arm-linux-musleabihf-cross /opt/
    echo 'export PATH=/opt/arm-linux-musleabihf-cross/bin:$PATH' >> ~/.bashrc
    source ~/.bashrc

Arm

    wget http://more.musl.cc/$(uname -m)-linux-musl/arm-linux-musleabi-cross.tgz
    tar -xvf arm-linux-musleabi-cross.tgz
    sudo mv arm-linux-musleabi-cross /opt/
    echo 'export PATH=/opt/arm-linux-musleabi-cross/bin:$PATH' >> ~/.bashrc
    source ~/.bashrc

Arm64

    wget http://more.musl.cc/$(uname -m)-linux-musl/aarch64-linux-musl-cross.tgz
    tar -xvf aarch64-linux-musl-cross.tgz
    sudo mv aarch64-linux-musl-cross /opt/
    echo 'export PATH=/opt/aarch64-linux-musl-cross/bin:$PATH' >> ~/.bashrc
    source ~/.bashrc

Install rust and arm/armv7/arm64 target

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    rustup update
    rustup target add arm-unknown-linux-musleabi        # arm
    rustup target add arm-unknown-linux-musleabihf      # armv6
    rustup target add armv7-unknown-linux-musleabihf    # armv7
    rustup target add aarch64-unknown-linux-musl        # arm64

Build

    cargo build --target arm-unknown-linux-musleabi --release       # arm
    cargo build --target arm-unknown-linux-musleabihf --release     # armv6
    cargo build --target armv7-unknown-linux-musleabihf --release   # armv7
    cargo build --target aarch64-unknown-linux-musl                 # arm64

## Cross compilation - windows

Install WSL and follow the linux cross compilation steps.

## Build web content

Build web content

    (cd electron && npm install && npm run build:web)

Try other mirrors when electron could not be downloaded

    ELECTRON_MIRROR="https://npm.taobao.org/mirrors/electron/" npm install

## Build and install deb packages

Build deb with cargo-deb (need latest cargo-deb that support templates)

    cargo install --git https://github.com/mmstick/cargo-deb.git

    cargo deb --target arm-unknown-linux-musleabi --manifest-path=pisugar-server/Cargo.toml
    cargo deb --target arm-unknown-linux-musleabi --manifest-path=pisugar-poweroff/Cargo.toml
    cargo deb --target arm-unknown-linux-musleabihf --manifest-path=pisugar-server/Cargo.toml
    cargo deb --target arm-unknown-linux-musleabihf --manifest-path=pisugar-poweroff/Cargo.toml
    cargo deb --target aarch64-unknown-linux-musl --manifest-path=pisugar-server/Cargo.toml
    cargo deb --target aarch64-unknown-linux-musl --manifest-path=pisugar-poweroff/Cargo.toml

Install

    # Install
    sudo dpkg -i pisugar-xxx_<version>_<arch>.deb

    # Uninstall/Purge
    sudo dpkg -P pisugar-xxx

To reconfigure after installation

    sudo dpkg-reconfigure pisugar-server
    sudo dpkg-reconfigure pisugar-poweroff

To preconfigure before installation

    sudo dpkg-preconfigure pisugar-server_<ver>_<arch>.deb
    sudo dpkg-preconfigure pisugar-poweroff_<ver>_<arch>.deb

## Build rpm packages

Install rpm on debian-like :

    sudo apt install rpm

Install cargo-rpm

    cargo install cargo-rpm

Build

    cargo rpm build --target arm-unknown-linux-musleabi
    cargo rpm build --target arm-unknown-linux-musleabihf

Install

    rpm -i pisugar-server-<ver>-<arch>.rpm

## Build aur packages (ArchLinux)

Build, ArchLinux only

    (cd scripts/aur; sh build-aur.sh)

Install

    sudo pacman -Sy binutils make gcc pkg-config fakeroot
    tar -xvf pisugar-all_<version>_all.tar.gz
    makepkg -si
