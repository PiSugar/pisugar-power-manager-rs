#!/usr/bin/env bash

set -e

CUR_DIR=$(cd $(dirname $0); pwd)
ROOT_DIR=$(cd "$CUR_DIR/../.."; pwd)

rm -rf *.tar.gz
rm -rf pisugar-archlinux
mkdir -p pisugar-archlinux
cp pisugar-bin.install pisugar-archlinux/
cp PKGBUILD pisugar-archlinux/
cp install.sh pisugar-archlinux/

cd pisugar-archlinux

version=$(cat PKGBUILD | grep ^pkgver | awk -F = '{print $2}')


getopt=$(which getopt)
ARGS=$($getopt -q -o b -l build -- "$@")
if [ $? != 0 ]; then
    exit 1
fi

eval set -- "${ARGS}"

build="N"

while true
do
    case "$1" in
        -b|--build)
            shift && build="Y"
            ;;
        --)
            shift && break
            ;;
        *)
            exit 1
            ;;
    esac
    shift
done

for i  in arm-unknown-linux-musleabi arm-unknown-linux-musleabihf aarch64-unknown-linux-musl x86_64-unknown-linux-musl; do
    if ! test -d "$ROOT_DIR/target/$i" || test "$build" = "Y"; then
      echo "Building $i"
      rustup target add $i
      (cd $ROOT_DIR; cargo build --target $i --release)
    fi
done

mkdir arm
mkdir armhf
mkdir aarch64
mkdir x86_64

for i in server poweroff programmer; do
    cp "$ROOT_DIR/target/arm-unknown-linux-musleabi/release/pisugar-$i" arm/
    cp "$ROOT_DIR/target/arm-unknown-linux-musleabihf/release/pisugar-$i" armhf/
    cp "$ROOT_DIR/target/aarch64-unknown-linux-musl/release/pisugar-$i" aarch64/
    cp "$ROOT_DIR/target/x86_64-unknown-linux-musl/release/pisugar-$i" x86_64/
done

for i in arm armhf aarch64 x86_64; do
    cp -r "$ROOT_DIR/pisugar-server/.rpm/_ws.json" $i/
    cp -r "$ROOT_DIR/pisugar-server/.rpm/config.json" $i/
    cp -r "$ROOT_DIR/pisugar-server/.rpm/pisugar-server.default" $i/
    cp -r "$ROOT_DIR/pisugar-server/.rpm/pisugar-server.service" $i/
    cp -r "$ROOT_DIR/pisugar-webui/dist/" $i/web/
    cp -r "$ROOT_DIR/pisugar-poweroff/.rpm/pisugar-poweroff.default" $i/
    cp -r "$ROOT_DIR/pisugar-poweroff/.rpm/pisugar-poweroff.service" $i/
done

tar -czvf pisugar-bin_${version}_all.tar.gz arm/ armhf/ aarch64/ x86_64/
rm -rf arm armhf aarch64 x86_64

echo "sha256sums=('$(sha256sum pisugar-bin_${version}_all.tar.gz | awk '{print $1}')')" >> PKGBUILD

(cd "$CUR_DIR"; tar -czvf pisugar-archlinux_${version}_all.tar.gz pisugar-archlinux)

rm -rf PKGBUILD pisugar-bin_${version}_all.tar.gz

cd "$CUR_DIR"
rm -rf pisugar-archlinux