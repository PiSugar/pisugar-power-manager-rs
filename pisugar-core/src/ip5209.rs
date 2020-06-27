use rppal::i2c::I2c;

use crate::{convert_battery_voltage_to_level, BatteryThreshold, Result};

/// Battery threshold curve
pub const BATTERY_CURVE: [BatteryThreshold; 10] = [
    (4.16, 100.0),
    (4.05, 95.0),
    (4.00, 80.0),
    (3.92, 65.0),
    (3.86, 40.0),
    (3.79, 25.5),
    (3.66, 10.0),
    (3.52, 6.5),
    (3.49, 3.2),
    (3.1, 0.0),
];

/// Idle intensity
const PI_ZERO_IDLE_INTENSITY: f64 = 0.11;

/// IP5209, pi-zero bat chip
pub struct IP5209 {
    i2c: I2c,
}

impl IP5209 {
    /// Create new IP5209
    pub fn new(i2c_addr: u16) -> Result<Self> {
        let mut i2c = I2c::new()?;
        i2c.set_slave_address(i2c_addr)?;
        Ok(Self { i2c })
    }

    /// Read voltage (V)
    pub fn read_voltage(&self) -> Result<f64> {
        let low = self.i2c.smbus_read_byte(0xa2)? as u16;
        let high = self.i2c.smbus_read_byte(0xa3)? as u16;

        // check negative values
        let voltage = if high & 0x20 == 0x20 {
            let v = (((high | 0b1100_0000) << 8) + low) as i16;
            2600.0 - (v as f64) * 0.26855
        } else {
            let v = ((high & 0x1f) << 8) + low;
            2600.0 + (v as f64) * 0.26855
        };

        Ok(voltage / 1000.0)
    }

    /// Parse level(%)
    pub fn parse_voltage_level(voltage: f64) -> f64 {
        let level = if voltage > 0.0 {
            convert_battery_voltage_to_level(voltage, &BATTERY_CURVE)
        } else {
            100.0
        };
        level
    }

    /// Read intensity (A)
    pub fn read_intensity(&self) -> Result<f64> {
        let low = self.i2c.smbus_read_byte(0xa4)? as u16;
        let high = self.i2c.smbus_read_byte(0xa5)? as u16;

        // check negative value
        let intensity = if high & 0x20 == 0x20 {
            let i = (((high | 0b1100_0000) << 8) + low) as i16;
            (i as f64) * 0.745985
        } else {
            let i = ((high & 0x1f) << 8) + low;
            (i as f64) * 0.745985
        };

        Ok(intensity / 1000.0)
    }

    /// Shutdown under light load (144mA and 8s)
    pub fn init_auto_shutdown(&self) -> Result<()> {
        let threshold = PI_ZERO_IDLE_INTENSITY * 1000.0;
        let threshold = (threshold / 12.0) as u64;
        let threshold = if threshold > 0b0001_1111 {
            0b0001_1111 as u8
        } else {
            threshold as u8
        };

        // threshold intensity, x*12mA = 108mA
        let mut v = self.i2c.smbus_read_byte(0x0c)?;
        v &= 0b0000_0111;
        v |= threshold << 3;
        self.i2c.smbus_write_byte(0x0c, v)?;

        // time, 8s
        let mut v = self.i2c.smbus_read_byte(0x04)?;
        v &= 0b00111111;
        self.i2c.smbus_write_byte(0x04, v)?;

        // enable auto shutdown and turn on
        let mut v = self.i2c.smbus_read_byte(0x02)?;
        v |= 0b0000_0011;
        self.i2c.smbus_write_byte(0x02, v)?;

        Ok(())
    }

    /// Enable gpio
    pub fn init_gpio(&self) -> Result<()> {
        // vset
        let mut v = self.i2c.smbus_read_byte(0x26)?;
        v |= 0b0000_0000;
        v &= 0b1011_1111;
        self.i2c.smbus_write_byte(0x26, v)?;

        // vset -> gpio
        let mut v = self.i2c.smbus_read_byte(0x52)?;
        v |= 0b0000_0100;
        v &= 0b1111_0111;
        self.i2c.smbus_write_byte(0x52, v)?;

        // enable gpio input
        let mut v = self.i2c.smbus_read_byte(0x53)?;
        v |= 0b0001_0000;
        v &= 0b1111_1111;
        self.i2c.smbus_write_byte(0x53, v)?;

        Ok(())
    }

    /// Init gpio, 2 led version
    pub fn init_gpio_2led(&self) -> Result<()> {
        // gpio1 tap, L4 sel
        let mut v = self.i2c.smbus_read_byte(0x51)?;
        v &= 0b1111_0011;
        v |= 0b0000_0100;
        self.i2c.smbus_write_byte(0x51, v)?;

        // gpio1 input enable
        let mut v = self.i2c.smbus_read_byte(0x53)?;
        v |= 0b0000_0010;
        self.i2c.smbus_write_byte(0x53, v)?;

        // charging control, gpio2
        let mut v = self.i2c.smbus_read_byte(0x51)?;
        v &= 0b1100_1111;
        v |= 0b1101_1111;
        self.i2c.smbus_write_byte(0x51, v)?;

        // vset -> register
        let mut v = self.i2c.smbus_read_byte(0x26)?;
        v &= 0b1011_0000;
        self.i2c.smbus_write_byte(0x26, v)?;

        // vset -> gpio4
        let mut v = self.i2c.smbus_read_byte(0x52)?;
        v &= 0b1111_0011;
        v |= 0b0000_0100;
        self.i2c.smbus_write_byte(0x52, v)?;

        // gpio4 input enable
        let mut v = self.i2c.smbus_read_byte(0x53)?;
        v &= 0b1110_1111;
        v |= 0b0001_0000;
        self.i2c.smbus_write_byte(0x53, v);

        Ok(())
    }

    /// Enable/Disable charging, 2 led version
    pub fn toggle_charging_2led(&self, enable: bool) -> Result<()> {
        // disable gpio2 output
        let mut v = self.i2c.smbus_read_byte(0x54)?;
        v &= 0b1111_1011;
        self.i2c.smbus_write_byte(0x54, v)?;

        // enable or disable charging
        let mut v = self.i2c.smbus_read_byte(0x55)?;
        v &= 0b1111_1011;
        if enable {
            v |= 0b0000_0100;
        }
        self.i2c.smbus_write_byte(0x55, v)?;

        // enable gpio2 output
        let mut v = self.i2c.smbus_read_byte(0x54)?;
        v &= 0b1111_1011;
        v |= 0b0000_0100;
        self.i2c.smbus_write_byte(0x54, v);

        Ok(())
    }

    pub fn is_charging_2led(&self) -> Result<bool> {
        let v = self.i2c.smbus_read_byte(0x55)?;
        if v & 0b0001_0000 != 0 {
            return Ok(true);
        }
        Ok(false)
    }

    /// Read gpio tap 4:0
    pub fn read_gpio_tap(&self) -> Result<u8> {
        let v = self.i2c.smbus_read_byte(0x55)?;
        Ok(v)
    }

    /// Force shutdown
    pub fn force_shutdown(&self) -> Result<()> {
        // force shutdown
        let mut t = self.i2c.smbus_read_byte(0x01)?;
        t &= 0b1111_1011;
        self.i2c.smbus_write_byte(0x01, t)?;

        Ok(())
    }
}
