# PiSugar 3

## I2C address

** PLEASE MAKE SURE YOU KNOW WHAT YOU ARE DOING **

The default i2c address of PiSugar 3 is 0x57. If this address is conflict with other devices, you could change PiSugar 3 i2c address.

But modify the i2c address is dangerous, it may damage the firmware data. So STOP reading if you feel the consequence is unacceptable.

Check whether the firmware could handle 0x50 (i2c address modification) or not:

    i2cget -y 1 0x57 0x50

If the response data is zero, that means the firmware need to be replaced with the latest version.

The how-to steps:

1. Choose an i2c address, 7bits only (bit6, bit5 ... bit0, less than 127)
2. Calculate the parity check bit

        check_bit = bit6 xor bit5 xor bit4 xor bit3 xor bit2 xor bit1 xor bit0
        value = addr | (check_bit << 7)

        e.g.
        addr        binary          check_bit   value_binary    value
        0x57 (87)   0b101_0111      1           0b1101_0111     0xD7
        0x75 (117)  0b111_0101      1           0b1111_0101     0xF5

3. Modify i2c address
    
        i2c set -y 1 <old i2c address> 0x50 <new value with check bit, e.g. 0xD7>

        e.g.
        i2c set -y 1 0x57 0x50 0xD7
        i2c set -y 1 0x57 0x50 0xF5

4. Modify `pisugar-server` configuration

    `/etc/pisugar-server/config.json`, e.g.

        {
            ...
            "i2c_addr": 117
            ...
        }

5. Restart `pisugar-server`

    sudo systemctl restart pisugar-server

Troubleshooting:

1. `i2cdetect` to detect new i2c address
2. Download a new application firmware to fix the problems
    
    Contact pisugar support team to get the firmware files

    Stop pisugar-server and upgrade pisugar3

        sudo systemctl stop pisugar-server
        pisugar-programmer -r --addr 0x57 --file pisugar-3-application.bin

## Write protection

PiSugar 3 firmware 1.24 add a new feature, i2c write protection, to avoid i2c data corruption. To modify the i2c data, you need to 
send a 0x29 to i2c 0x0b, then update other i2c data, and finally send a 0x00 to i2c 0x0b to end the process.