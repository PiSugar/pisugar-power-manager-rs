# PiSugar Linux kernel modules

Linux kernel modules for PiSugar 3.

## Preparing for building RPI kernel module

### Linux distributions with kernel symbols

Congratulations if your PI is running a linux distribution that has `/lib/modules/$(uname -r)/build/` directory, e.g. ubuntu-server or latest pi os, you don't need to manually build RPI kernel, and that will save a lot of time.

Install `build-essential` and `linux-headers`
```shell
sudo apt install -y build-essential linux-headers-$(uname -r)
```

Running 32bit OS in a 64bit machine, see [this](https://forums.raspberrypi.com/viewtopic.php?t=367669).

### Old raspberry Pi OS

As kernel symbols is not included in Raspberry Pi OS (no `/lib/modules/$(uname -r)/build`), so you need to compile the kernel and generate the kernel symbols by yourself. 

To build the kernel, see official doc: https://www.raspberrypi.com/documentation/computers/linux_kernel.html

## Compile/install/uninstall kernel module

Clone this repository, make kernel modules:
```shell
make
```

Install:
```shell
sudo make install
```

Install with parameters:
```shell
sudo make install i2c_bus=0x01 i2c_addr=0x57
```

Uninstall:
```shell
sudo make uninstall
```

## Manually load kernel module

Load module:
```shell
sudo insmod pisugar_3_battery.ko
# or
sudo insmod pisugar_3_battery.ko i2c_bus=1 i2c_addr=0x57
```

Now, it is loaded:
```shell
lsmod | grep battery
```

And you will see extra device files in `/sys/class/power_supply`
```shell
ls /sys/class/power_supply
```

Remove module:
```shell
sudo rmmod pisugar_3_battery.ko
```

Now, you can enable a battery monitor plugin that reads battery status from power supply subsystem (OS battery monitor plugin or a 3rd party plugin).

If you want to load kernel module at boot time, copy it to `/lib/modules/$(uname -r)/kernel/drivers/power/supply`
```shell
sudo cp -f pisugar_3_battery.ko /lib/modules/$(uname -r)/kernel/drivers/power/supply
sudo echo pisugar_3_battery >> /etc/modules
sudo depmod -a
```

You may want to change module parameters:
```shell
echo "options pisugar_3_battery i2c_bus=0x01 i2c_addr=0x57" | sudo tee /etc/modprobe.d/pisugar_3_battery.conf
```

## License

GPL