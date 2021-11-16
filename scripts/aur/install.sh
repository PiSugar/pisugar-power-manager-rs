#!/bin/sh

set -e

version=$(cat PKGBUILD | grep ^pkgver | awk -F = '{print $2}')

sudo pacman -Sy binutils make gcc pkg-config fakeroot

makepkg -si

echo "PiSugar hardware model:"
select model in "PiSugar 3" "PiSugar 2 Pro" "PiSugar 2 (2-LEDs)" "PiSugar 2 (4-LEDs)"; do
    if [ "x$model" != "x" ]; then
        break;
    fi
done

echo "Model: $model"
sudo sed -e "s/--model.*--/--model '$model' --/" \
    -i /etc/default/pisugar-server.default \
    -i /etc/default/pisugar-poweroff.default \
    || true

sudo systemctl enable pisugar-server || true
sudo systemctl start pisugar-server || true
