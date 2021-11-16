#!/usr/bin/env bash

set -e

CUR_DIR=$(cd $(dirname $0); pwd)
ROOT_DIR=$(cd "$CUR_DIR/../.."; pwd)

rm -rf *.tar.gz
rm -rf pisugar-all
mkdir -p pisugar-all
cp pisugar-bin.install pisugar-all/
cp PKGBUILD pisugar-all/
cp install.sh pisugar-all/

cd pisugar-all

version=$(cat PKGBUILD | grep ^pkgver | awk -F = '{print $2}')

(cd $ROOT_DIR; cargo build --target arm-unknown-linux-musleabi  --release )
(cd $ROOT_DIR; cargo build --target arm-unknown-linux-musleabihf  --release )
(cd $ROOT_DIR; cargo build --target aarch64-unknown-linux-musl  --release )

mkdir arm
mkdir armhf
mkdir aarch64

for i in server poweroff programmer; do
    cp "$ROOT_DIR/target/arm-unknown-linux-musleabi/release/pisugar-$i" arm/
    cp "$ROOT_DIR/target/arm-unknown-linux-musleabihf/release/pisugar-$i" armhf/
    cp "$ROOT_DIR/target/aarch64-unknown-linux-musl/release/pisugar-$i" aarch64/ 
done

for i in arm armhf aarch64; do
    cp -r "$ROOT_DIR/pisugar-server/.rpm/_ws.json" $i/
    cp -r "$ROOT_DIR/pisugar-server/.rpm/config.json" $i/
    cp -r "$ROOT_DIR/pisugar-server/.rpm/pisugar-server.default" $i/
    cp -r "$ROOT_DIR/pisugar-server/.rpm/pisugar-server.service" $i/
    cp -r "$ROOT_DIR/electron/dist/web" $i/
    cp -r "$ROOT_DIR/pisugar-poweroff/.rpm/pisugar-poweroff.default" $i/
    cp -r "$ROOT_DIR/pisugar-poweroff/.rpm/pisugar-poweroff.service" $i/
done

tar -czvf pisugar-bin_${version}_all.tar.gz arm/ armhf/ aarch64/

echo "sha256sums=('$(sha256sum pisugar-bin_${version}_all.tar.gz | awk '{print $1}')')" >> PKGBUILD

rm -rf arm armhf aarch64

(cd "$CUR_DIR"; tar -czvf pisugar-all_${version}_all.tar.gz pisugar-all)

rm -rf PKGBUILD pisugar-bin_${version}_all.tar.gz

cd "$CUR_DIR"
rm -rf pisugar-all
