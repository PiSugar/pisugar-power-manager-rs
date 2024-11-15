TARGET = pisugar_2_battery

obj-m += pisugar_2_battery.o

KERN_VER = $(shell uname -r)

SOURCEDIR = $(shell pwd)
BUILDDIR = $(shell pwd)/build

I2C_BUS ?= 0x01
I2C_ADDR ?= 0x75
BAT_MODEL ?= 0

all:
	make -C /lib/modules/$(KERN_VER)/build M=$(SOURCEDIR) modules

clean:
	rm -f *.cmd *.ko *.o Module.symvers modules.order *.mod.c .pisugar_2_battery.* .modules.* .Module.symvers.* pisugar_2_battery.mod

install: all
	cp -f pisugar_2_battery.ko /lib/modules/$(KERN_VER)/kernel/drivers/power/supply
	grep -q pisugar_2_battery /etc/modules || sed -i '$$a pisugar_2_battery' /etc/modules
	echo "options pisugar_2_battery i2c_bus=$(I2C_BUS) i2c_addr=$(I2C_ADDR) bat_module=$(BAT_MODEL)" > /etc/modprobe.d/pisugar_2_battery.conf
	depmod -a
	modprobe pisugar_2_battery

uninstall:
	rmmod pisugar_2_battery || true
	rm -f /lib/modules/$(KERN_VER)/kernel/drivers/power/supply/pisugar_2_battery.ko
	sed -i '/pisugar_2_battery/d' /etc/modules
	rm -f /etc/modprobe.d/pisugar_2_battery.conf
