#!/bin/bash
# Simplified version: Disable write protection → Write address → Enable write protection

ADDR=0x68  # Set the target address directly in the script (7-bit address, 0x08-0x77, if use 0x57,RTC address will be disable)

# Configuration variables
BUS=1
DEV=0x57
WP_REG=0x0B
WP_OFF=0x29
WP_ON=0x00
RTC_CHANGE_ADD=0x51

# Function to calculate even parity
parity() {
    local v=$1 c=0
    for i in {0..6}; do (( c += (v >> i) & 1 )); done
    (( c % 2 )) && echo $(( v | 0x80 )) || echo "$v"
}

# Check if the address is valid
if [ "$ADDR" -eq "0x57" ]; then
    echo "Error: 0x57 is a reserved address and cannot be used."
    exit 1
fi

# Disable write protection
echo "Disabling write protection..."
i2cset -y $BUS $DEV $WP_REG $WP_OFF w || { echo "Failed to disable write protection"; exit 1; }
sleep 0.1

# Write the address with parity
echo "Writing address: 0x$(printf "%02x" $ADDR)"
VAL=$(parity $ADDR)
i2cset -y $BUS $DEV $RTC_CHANGE_ADD $VAL w || { echo "Write failed"; exit 1; }
sleep 0.1

# Enable write protection
echo "Re-enabling write protection..."
i2cset -y $BUS $DEV $WP_REG $WP_ON w || { echo "Failed to enable write protection"; exit 1; }

echo "Completed: Address 0x$(printf "%02x" $ADDR) has been written and write protection has been re-enabled."