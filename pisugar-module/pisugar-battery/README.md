# PiSugar battery kernel module

`pisugar_battery` detects PiSugar 3 (`0x57`) or PiSugar 2 (`0x75`) and
registers `BAT0` and `AC0` through Linux's `power_supply` subsystem. Desktop
battery plugins can then display the PiSugar battery icon and status.

The normal installer downloads a module prebuilt for the exact running kernel;
users do not need kernel headers or a compiler. Because Linux kernel modules
are ABI-specific, update Raspberry Pi OS when the installer reports that no
package exists for the running `uname -r` value.

Detection is automatic. On a PiSugar 2 where both chip register layouts happen
to return plausible data, `pisugar2_model=0` forces standard/IP5209 and
`pisugar2_model=1` forces Pro/IP5312. PiSugar 3 uses `0x57` by default. The
installer reads `i2c_bus` and `i2c_addr` from an installed PiSugar Server's
configuration; for another custom address, set the `pisugar3_addr` module
parameter.

For development only, build against an explicitly selected kernel tree:

```sh
make KERNEL_DIR=/lib/modules/$(uname -r)/build
```
