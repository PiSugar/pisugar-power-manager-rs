# PiSugar Linux kernel modules

Linux kernel modules for PiSugar 3.

## Preparing for building RPI kernel module

### Linux distributions with kernel symbols

Congratulations if your PI is running a linux distribution that has `/lib/modules/$(uname -r)/build/` directory, e.g. ubuntu-server or latest pi os, you don't need to manually build RPI kernel, and that will save a lot of time.

Install `build-essential` and `linux-headers`
```shell
sudo apt install -y build-essential linux-headers-$(uname -r)
```

### Old raspberry Pi OS

As kernel symbols is not included in Raspberry Pi OS (no `/lib/modules/$(uname -r)/build`), so you need to compile the kernel and generate the kernel symbols by yourself. 

To build the kernel, see official doc: https://www.raspberrypi.com/documentation/computers/linux_kernel.html

## Compiling kernel module

Clone this repository, make kernel modules:
```shell
make
```

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

Now, you can enable [battery monitor plugin](https://github.com/raspberrypi-ui/lxplug-ptbatt).

If you want to load kernel module at boot time, copy it to `/lib/modules/$(uname -r)/kernel/drivers`
```shell
sudo cp -f pisugar_3_battery.ko /lib/modules/$(uname -r)/kernel/drivers
sudo echo pisugar_3_battery >> /etc/modules
sudo depmod -a
```

## License

GPL