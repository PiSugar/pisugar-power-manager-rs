#!/bin/sh

set -e

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

rustup target add arm-unknown-linux-musleabi
rustup target add arm-unknown-linux-musleabihf
rustup target add armv7-unknown-linux-musleabihf
rustup target add aarch64-unknown-linux-musl

for i in arm-linux-musleabi-cross arm-linux-musleabihf-cross aarch64-linux-musl-cross; do
    if ! test -d /opt/${i}/bin; then
        wget https://more.musl.cc/$(uname -m)-linux-musl/${i}.tgz
        tar -xvf ${i}.tgz
        sudo mv $i /opt/
    fi

    if !(echo $PATH | grep "/opt/$i/bin" > /dev/null 2>&1); then
        echo 'export PATH=$PATH:/opt/'$i'/bin' >> ~/.profile
    fi
done
