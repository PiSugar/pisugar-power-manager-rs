# PiSugar Linux kernel modules

Linux kernel modules for PiSugar 3.

## Preparing for building RPI kernel module

### Linux distributions with kernel symbols

Congratulations if your PI is running a linux distribution that has `/lib/modules/$(uname -r)/build/` directory, e.g. ubuntu-server, you don't need to manually build RPI kernel, and that will save a lot of time.

Install linux-headers
```shell
sudo apt install linux-headers-$(uname -r)
```

### Raspberry Pi OS

As kernel symbols is not included in Raspberry Pi OS (no `/lib/modules/$(uname -r)/build`), so you need to compile the kernel and generate the kernel symbols by youself. 

To build the kernel, see official doc: https://www.raspberrypi.com/documentation/computers/linux_kernel.html

** It seems like the precompiled symbols could be downloaded from [here](https://github.com/raspberrypi/firmware), but I could not figure out the correct steps. **

First, get RPI OS tag, e.g. 1.20230405
```shell
dpkg -l | grep kernel
```

Clone kernel repository, and create a symbol link
```shell
git clone --depth 1 --branch <PI_OS_TAG> https://github.com/raspberrypi/linux.git
sudo ln -s "$(pwd)/linux" /usr/src/linux
```

Build RPI kernel.

When it is done, copy and rename `linux` folder to PI `/lib/modules/$(uname -r)/build`.

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

If you want to load kernel module at boot time, copy it to `/lib/modules/$(uname -r)/kernel/drivers`, then
```shell
sudo echo pisugar_3_battery >> /etc/modules
sudo depmod -a
```

## License

GPL