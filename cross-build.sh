#!/bin/bash

set -e
set -o pipefail

TARGET=armv7-unknown-linux-musleabihf

function print_usage() {
    echo "Usage: $0 [options]"
    echo ""
    echo "Options:"
    echo "  -h            Show this help message and exit"
    echo "  -t <target>   Specify the target architecture (default: $TARGET)"
    echo "                Available targets, see rustup target list (not all of them supported by cross)"
}

ARGS=$(getopt -q -o ht: --name "$0" -- "$@")
if [ $? != 0 ]; then
    print_usage && exit 1
fi
eval set -- "${ARGS}"

while true
do
    case "$1" in
        -h)
            print_usage && exit 0
            ;;
        -t)
            shift && TARGET=$1
            ;;
        --)
            shift
            break
            ;;
        *)
            print_usage && exit 1
            ;;
    esac
    shift
done

if !which cross &> /dev/null; then
    echo "Error: cross is not installed. Please install it with "cargo install cross --git https://github.com/cross-rs/cross""
fi

cross build --target $TARGET --release
for app in server poweroff programmer; do
  cargo deb --no-build --no-strip --target ${TARGET} -p pisugar-${app}
  cargo generate-rpm --target $TARGET -p pisugar-${app}
done

mkdir -p ${TARGET}
cp target/${TARGET}/release/pisugar-server ${TARGET}
cp target/${TARGET}/release/pisugar-poweroff ${TARGET}
cp target/${TARGET}/release/pisugar-programmer ${TARGET}

cp scripts/install.sh ${TARGET}

mkdir -p ${TARGET}/pisugar-server-conf
cp pisugar-server/debian/pisugar-server.default ${TARGET}/pisugar-server-conf/
cp pisugar-server/debian/pisugar-server.service ${TARGET}/pisugar-server-conf/
cp pisugar-server/debian/config.json ${TARGET}/pisugar-server-conf/

cp -R pisugar-webui/dist ${TARGET}/web-ui

mkdir -p ${TARGET}/pisugar-poweroff-conf
cp pisugar-poweroff/debian/pisugar-poweroff.default ${TARGET}/pisugar-poweroff-conf/
cp pisugar-poweroff/debian/pisugar-poweroff.service ${TARGET}/pisugar-poweroff-conf/

tar -czvf pisugar_${TARGET}.tar.gz ${TARGET}
