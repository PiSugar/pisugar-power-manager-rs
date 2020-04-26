use rppal::i2c::I2c;

use crate::Result;
use crate::{convert_battery_voltage_to_level, I2cError};
use crate::{BatteryThreshold, Error};

/// Battery threshold curve
pub const BATTERY_CURVE: [BatteryThreshold; 10] = [
    (4.16, 100.0),
    (4.05, 95.0),
    (3.90, 88.0),
    (3.80, 77.0),
    (3.70, 65.0),
    (3.62, 55.0),
    (3.58, 49.0),
    (3.49, 25.6),
    (3.32, 4.5),
    (3.1, 0.0),
];

/// Idle intensity
const PI_PRO_IDLE_INTENSITY: f64 = 0.25;

/// IP5312, pi-3/4 bat chip
pub struct IP5312 {
    i2c: I2c,
}

impl IP5312 {
    /// Create new IP5312
    pub fn new(i2c_addr: u16) -> Result<Self> {
        let mut i2c = I2c::new()?;
        i2c.set_slave_address(i2c_addr)?;
        Ok(Self { i2c })
    }

    /// Read voltage (V)
    pub fn read_voltage(&self) -> Result<f64> {
        let low = self.i2c.smbus_read_byte(0xd0)? as u16;
        let high = self.i2c.smbus_read_byte(0xd1)? as u16;

        if low == 0 && high == 0 {
            return Err(Error::I2c(I2cError::FeatureNotSupported));
        }

        let v = ((high & 0b0011_1111) << 8) + low;
        let v = (v as f64) * 0.26855 + 2600.0;
        Ok(v / 1000.0)
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
        let low = self.i2c.smbus_read_byte(0xd2)? as u16;
        let high = self.i2c.smbus_read_byte(0xd3)? as u16;

        let intensity = if high & 0x20 != 0 {
            let i = (((high | 0b1100_0000) << 8) + low) as i16;
            (i as f64) * 2.68554
        } else {
            let i = ((high & 0x1f) << 8) + low;
            (i as f64) * 2.68554
        };
        Ok(intensity / 1000.0)
    }

    /// Shutdown under light load (126mA and 8s)
    pub fn init_auto_shutdown(&self) -> Result<()> {
        let threshold = PI_PRO_IDLE_INTENSITY * 1000.0;
        let threshold = (threshold / 4.3) as u64;
        let threshold = if threshold > 0b0011_1111 {
            0b0011_1111 as u8
        } else {
            threshold as u8
        };

        // threshold intensity, x*4.3mA = 126mA
        let mut v = self.i2c.smbus_read_byte(0xc9)?;
        v &= 0b1100_0000;
        v |= threshold;
        self.i2c.smbus_write_byte(0xc9, v)?;

        // time, 8s
        let mut v = self.i2c.smbus_read_byte(0x06)?;
        v &= 0b0011_1111;
        self.i2c.smbus_write_byte(0x06, v)?;

        // enable
        let mut v = self.i2c.smbus_read_byte(0x03)?;
        v |= 0b0010_0000;
        self.i2c.smbus_write_byte(0x03, v)?;

        // enable bat low, 2.76-2.84V
        let mut v = self.i2c.smbus_read_byte(0x13)?;
        v &= 0b1100_1111;
        v |= 0b0001_0000;
        self.i2c.smbus_write_byte(0x13, v)?;

        Ok(())
    }

    /// Enable gpio1
    pub fn init_gpio(&self) -> Result<()> {
        // mfp_ctl0, set l4_sel
        let mut v = self.i2c.smbus_read_byte(0x52)?;
        v |= 0b0000_0010;
        self.i2c.smbus_write_byte(0x52, v)?;

        // gpio1 input
        let mut v = self.i2c.smbus_read_byte(0x54)?;
        v |= 0b0000_0010;
        self.i2c.smbus_write_byte(0x54, v)?;

        Ok(())
    }

    /// Read gpio tap
    pub fn read_gpio_tap(&self) -> Result<u8> {
        let mut v = self.i2c.smbus_read_byte(0x58)?;
        v &= 0b0000_0010;

        Ok(v)
    }

    /// Force shutdown
    pub fn force_shutdown(&self) -> Result<()> {
        // enable force shutdown
        let mut t = self.i2c.smbus_read_byte(0x01)?;
        t &= 0b1111_1011;
        self.i2c.smbus_write_byte(0x01, t)?;

        Ok(())
    }
}
