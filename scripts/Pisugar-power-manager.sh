#!/bin/bash
set -e

version=1.4.5
channel=release

prog="$0"

function usage() {
    cat << EOF
$prog

Install pisugar power manager tools.

USAGE: $prog [OPTIONS]

OPTIONS:
    -h|--help       Print this usage.
    -v|--version    Install a specified version, default: $version
    -c|--channel    Choose nightly or release channel, default: $channel

For more details, see https://github.com/PiSugar/pisugar-power-manager-rs

EOF
}

echo=$(which echo)
getopt=$(which getopt)
ARGS=$($getopt -q -o hv:c: -l help,version:,channel: -- "$@")
if [ $? != 0 ]; then
    usage && exit 1
fi

eval set -- "${ARGS}"

while true
do
    case "$1" in
        -h|--help)
            usage && exit 0
            ;;
        -v|--version)
            shift && version=$1
            ;;
        -c|--channel)
            shift && channel=$1
            ;;
        --)
            shift && break
            ;;
        *)
            usage && exit 1
            ;;
    esac
    shift
done


package_server="pisugar-server_${version}_armhf.deb"
package_poweroff="pisugar-poweroff_${version}_armhf.deb"
local_host="$(hostname --fqdn)"
local_ip=$(ip addr |grep inet |grep -v inet6 |grep wlan0|awk '{print $2}' |awk -F "/" '{print $1}')

$echo -e "\033[1;34mDownload PiSugar-server and PiSugar-poweroff package \033[0m"
wget -O "/tmp/$package_server" "http://cdn.pisugar.com/${channel}/${package_server}"
wget -O "/tmp/$package_poweroff" "http://cdn.pisugar.com/${channel}/${package_poweroff}"

$echo -e "\033[1;34mOpen I2C Interface \033[0m"
sudo raspi-config nonint do_i2c 0

$echo -e "\033[1;34mUninstall old packages if installed\033[0m"
sudo dpkg -r pisugar-server pisugar-poweroff

$echo -e "\033[1;34mInstall packages\033[0m"
sudo dpkg -i "/tmp/${package_server}"  "/tmp/${package_poweroff}"

$echo -e "\033[1;34mClean up \033[0m"
rm -f "/tmp/${package_server}"
rm -f "/tmp/${package_poweroff}"

$echo -e "Now navigate to \033[1;34mhttp://${local_ip}:8421\033[0m on your browser to see PiSugar power management"
$echo -e "If you have any question,please feel free to contact us."
$echo -e "\033[1;34mThe PiSugar Team https://www.pisugar.com\033[0m"
