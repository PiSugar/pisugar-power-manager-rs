#!/bin/bash

# --- Script Configuration ---
I2C_BUS=1         # I2C Bus number (usually 1 on Raspberry Pi)
I2C_ADDR=0x57     # PiSugar I2C Device Address (0x57 often used for PiSugar 3-series write protect)
REG_CONTROL=0x02  # Control register address (Bit 5 enables delayed shutdown)
REG_DELAY=0x09    # Delay time setting register address (0-255 seconds)
REG_WRITE_PROTECT=0x0b # Write Protect register address

DELAY_SECONDS=30  # **Shutdown Delay Time (0-255 seconds) - Modify this value as needed**

# Check if i2c-tools is installed
if ! command -v i2cset &> /dev/null || ! command -v i2cget &> /dev/null; then
    echo "Error: i2cget or i2cset command not found. Please install i2c-tools package."
    exit 1
fi

# Check delay range
if (( DELAY_SECONDS < 0 || DELAY_SECONDS > 255 )); then
    echo "Error: Shutdown delay time ($DELAY_SECONDS seconds) is out of the valid range (0-255)."
    exit 1
fi

echo "--- PiSugar Delayed Shutdown Setup  ---"
echo "Target delay time: $DELAY_SECONDS seconds"
echo "-------------------------------------------------"

# 1. Disable Write Protection (Write 0x29 to 0x0b)
echo "1. Disabling write protection (0x0b <- 0x29)..."
sudo i2cset -y $I2C_BUS $I2C_ADDR $REG_WRITE_PROTECT 0x29

# 2. Read original value of 0x02 register
# 'b' flag reads one byte
ORIGINAL_VAL_HEX=$(sudo i2cget -y $I2C_BUS $I2C_ADDR $REG_CONTROL b)
echo "   Original 0x02 value: $ORIGINAL_VAL_HEX"

# 3. Set the desired delay time (Write DELAY_SECONDS to 0x09)
DELAY_HEX=$(printf "0x%x" $DELAY_SECONDS)
echo "2. Setting delay time to $REG_DELAY ($DELAY_HEX)..."
sudo i2cset -y $I2C_BUS $I2C_ADDR $REG_DELAY $DELAY_HEX

# 4. Calculate and enable delayed shutdown (Set Bit 5 of 0x02)
CLEAR_MASK_HEX=0xDF  # Mask to clear Bit 5
# NEW_VAL = ORIGINAL_VAL | BIT_MASK
NEW_VAL_DEC=$(( ORIGINAL_VAL_HEX & CLEAR_MASK_HEX ))
NEW_VAL_HEX=$(printf "0x%x" $NEW_VAL_DEC)

echo "3. Enabling delayed shutdown ($REG_CONTROL <- $NEW_VAL_HEX)..."
sudo i2cset -y $I2C_BUS $I2C_ADDR $REG_CONTROL $NEW_VAL_HEX

# 5. Enable Write Protection (Write 0x00 to 0x0b)
echo "4. Enabling write protection (0x0b <- 0x00)..."
sudo i2cset -y $I2C_BUS $I2C_ADDR $REG_WRITE_PROTECT 0x00

echo "--- Operation Completed ---"
echo "Delayed shutdown has been configured. Countdown starts when the system output is turned OFF."

sudo shutdown now