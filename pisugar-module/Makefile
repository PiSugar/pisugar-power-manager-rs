obj-m := rtc-sd3078.o
CURRENT_PATH := $(shell pwd)
LINUX_KERNEL := $(shell uname -r)
LINUX_KERNEL_PATH := /usr/src/linux-headers-$(LINUX_KERNEL)
BUILD_DIR := $(shell pwd)/build
BUILD_DIR_MAKEFILE := $(PWD)/build/Makefile

all: $(BUILD_DIR_MAKEFILE)
	$(MAKE) -C $(LINUX_KERNEL_PATH) M=$(BUILD_DIR) src=$(PWD) modules

$(BUILD_DIR):
	mkdir -p "$@"

$(BUILD_DIR_MAKEFILE): $(BUILD_DIR)
	touch "$@"

clean:
	$(MAKE) -C $(LINUX_KERNEL_PATH) M=$(BUILD_DIR) src=$(PWD) clean