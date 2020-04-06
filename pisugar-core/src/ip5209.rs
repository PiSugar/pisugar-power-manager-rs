use rppal::i2c::I2c;

use crate::Result;

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
        // threshold intensity, 9*12mA = 108mA
        let mut v = self.i2c.smbus_read_byte(0x0c)?;
        v &= 0b0000_0111;
        v |= 9 << 3;
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

    /// read gpio tap
    pub fn read_gpio_tap(&self) -> Result<u8> {
        let v = self.i2c.smbus_read_byte(0x55)?;
        Ok(v)
    }
}
