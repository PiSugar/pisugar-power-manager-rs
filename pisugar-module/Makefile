TARGET = pisugar_3_battery

obj-m += pisugar_3_battery.o

KERN_VER = $(shell uname -r)

SOURCEDIR = $(shell pwd)
BUILDDIR = $(shell pwd)/build

all:
	make -C /lib/modules/$(KERN_VER)/build M=$(SOURCEDIR) modules

clean:
	rm -f *.cmd *.ko *.o Module.symvers modules.order *.mod.c .pisugar_3_battery.* .modules.* .Module.symvers.* pisugar_3_battery.mod
