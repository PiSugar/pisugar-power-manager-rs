TARGET = pisugar_3_battery

obj-m += pisugar_3_battery.o

KERN_VER = $(shell uname -r)

SOURCEDIR = $(shell pwd)
BUILDDIR = $(shell pwd)/build

I2C_BUS ?= 0x01
I2C_ADDR ?= 0x57

all:
	make -C /lib/modules/$(KERN_VER)/build M=$(SOURCEDIR) modules

clean:
	rm -f *.cmd *.ko *.o Module.symvers modules.order *.mod.c .pisugar_3_battery.* .modules.* .Module.symvers.* pisugar_3_battery.mod

install: all
	cp -f pisugar_3_battery.ko /lib/modules/$(KERN_VER)/kernel/drivers/power/supply
	grep -q pisugar_3_battery /etc/modules || sed -i '$$a pisugar_3_battery' /etc/modules
	echo "options pisugar_3_battery i2c_bus=$(I2C_BUS) i2c_addr=$(I2C_ADDR)" > /etc/modprobe.d/pisugar_3_battery.conf
	depmod -a
	modprobe pisugar_3_battery

uninstall:
	rmmod pisugar_3_battery || true
	rm -f /lib/modules/$(KERN_VER)/kernel/drivers/power/supply/pisugar_3_battery.ko
	sed -i '/pisugar_3_battery/d' /etc/modules
	rm -f /etc/modprobe.d/pisugar_3_battery.conf
