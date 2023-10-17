#/bin/bash
echo "\033[1;34mClose I2C Interface \033[0m"
echo "\033[1;34mIf you stay here too long, please remove the PiSugar and manually close the I2C interface before using PiSugar S \033[0m"
sudo raspi-config nonint do_i2c 0

echo "\033[1;34mRegister GPIO BCM 3 \033[0m"
echo 3 >/sys/class/gpio/export
echo "\033[1;34mSet GPIO direction \033[0m"
sudo echo in >/sys/class/gpio/gpio3/direction
echo "\033[1;34mGet GPIO Value \033[0m"
while [ 1 ]; do
    #100ms
    sleep 0.1
    ButtonValue=$(cat /sys/class/gpio/gpio3/value)
    if [ $ButtonValue == 0 ]; then
        count=0
        echo "Push"
        while [ $ButtonValue == 0 ]; do
            ((count++))
            ButtonValue=$(cat /sys/class/gpio/gpio3/value)
            sleep 0.001
            if [ $count -gt 500 ]; then
                break 1
            fi
        done

        if [ $count -gt 500 ]; then
            echo "Charging"
            while [ $ButtonValue == 0 ]; do
                ButtonValue=$(cat /sys/class/gpio/gpio3/value)
                sleep 1
            done
            echo "Charg end"
        else
            echo $count
            if [ $count -gt 50 ]; then
                echo "Longclik"
            else
                echo "shortclik"
            fi
        fi
    fi
done
