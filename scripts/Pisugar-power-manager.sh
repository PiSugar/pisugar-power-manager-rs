#/bin/sh
set -e

echo=$(which echo)

package_server="pisugar-server_1.4.0_armhf.deb"
package_poweroff="pisugar-poweroff_1.4.0_armhf.deb"
local_host="`hostname --fqdn`"
local_ip=$(ip addr |grep inet |grep -v inet6 |grep wlan0|awk '{print $2}' |awk -F "/" '{print $1}')

$echo -e "\033[1;34mDownload PiSugar-server and PiSugar-poweroff package \033[0m"
wget -O /tmp/$package_server http://cdn.pisugar.com/release/${package_server}
wget -O /tmp/$package_poweroff http://cdn.pisugar.com/release/${package_poweroff}

$echo -e "\033[1;34mOpen I2C Interface \033[0m"
sudo raspi-config nonint do_i2c 0

$echo -e "\033[1;34mUninstall old packages if installed\033[0m"
sudo dpkg -r pisugar-server pisugar-poweroff

$echo -e "\033[1;34mInstall packages\033[0m"
sudo dpkg -i /tmp/${package_server}  /tmp/${package_poweroff}

$echo -e "\033[1;34mClean up \033[0m"
rm -f /tmp/${package_server}
rm -f /tmp/${package_poweroff}

$echo -e "Now navigate to \033[1;34mhttp://${local_ip}:8421\033[0m on your browser to see PiSugar power management"
$echo -e "If you have any question,please feel free to contact us."
$echo -e "\033[1;34mwww.PiSugar.com\033[0m"