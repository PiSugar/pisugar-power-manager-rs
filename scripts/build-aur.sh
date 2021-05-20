#!/usr/bin/env bash

set -e

CUR_DIR=$(cd $(dirname $0); pwd)
ROOT_DIR=$(cd "$CUR_DIR/.."; pwd)

version=$(cat PKGBUILD | grep pkgver | awk -F = '{print $2}')

for target_dir in arm-unknown-linux-musleabihf aarch64-unknown-linux-musl; do
  tar -czvf pisugar-bin-$version.tar.gz \
    "$ROOT_DIR/target/$target_dir/release/pisugar-server" \
    "$ROOT_DIR/pisugar-server/.rpm/_ws.json" \
    "$ROOT_DIR/pisugar-server/.rpm/config.json" \
    "$ROOT_DIR/pisugar-server/.rpm/pisugar-server.default" \
    "$ROOT_DIR/pisugar-server/.rpm/pisugar-server.service" \
    "$ROOT_DIR/electron/dist/web" \
    "$ROOT_DIR/target/$target_dir/release/pisugar-poweroff" \
    "$ROOT_DIR/pisugar-poweroff/.rpm/pisugar-poweroff.default" \
    "$ROOT_DIR/pisugar-server/.rpm/pisugar-poweroff.service"
done