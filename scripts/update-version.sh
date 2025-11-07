#!/usr/bin/env bash

set -e

CUR_DIR=$(cd $(dirname "$0"); pwd)
ROOT_DIR=$(cd "$CUR_DIR/.."; pwd)

if which gsed > /dev/null 2>&1; then
  # gsed on macos, to install: brew install gnu-sed
  SED=gsed
else
  SED=sed
fi

function usage() {
    echo "USAGE: $0 VERSION"
}

version=$1

if [ x"$version" == x"" ]; then
  usage && exit 1
fi

for dir in pisugar-server pisugar-core pisugar-poweroff pisugar-programmer; do
  $SED -e "s/^version[[:space:]]*=.*$/version = \"$version\"/" -i "$ROOT_DIR/$dir/Cargo.toml"
done

$SED -e "s/^version=.*/version=$version/" -i "$CUR_DIR/pisugar-power-manager.sh"
