use chrono::prelude::*;
use rppal::i2c::I2c;

use crate::rtc::{bcd_to_dec, dec_to_bcd, RTCRawTime, RTC};
use crate::{PiSugarConfig, Result};

/// SD3078, rtc chip
pub struct SD3078 {
    i2c: I2c,
}

impl SD3078 {
    /// Create new SD3078
    pub fn new(i2c_bus: u8, i2c_addr: u16) -> Result<Self> {
        let mut i2c = I2c::with_bus(i2c_bus)?;
        i2c.set_slave_address(i2c_addr)?;
        Ok(Self { i2c })
    }

    /// Disable write protect
    fn enable_write(&self) -> Result<()> {
        // ctr2 - wrtc1
        let mut crt2 = self.i2c.smbus_read_byte(0x10)?;
        crt2 |= 0b1000_0000;
        self.i2c.smbus_write_byte(0x10, crt2)?;

        // ctr1 - wrtc2 and wrtc3
        let mut crt2 = self.i2c.smbus_read_byte(0x0f)?;
        crt2 |= 0b1000_0100;
        self.i2c.smbus_write_byte(0x0f, crt2)?;

        Ok(())
    }

    /// Enable write protect
    fn disable_write(&self) -> Result<()> {
        // ctr1 - wrtc2 and wrtc3
        let mut crt1 = self.i2c.smbus_read_byte(0x0f)?;
        crt1 &= 0b0111_1011;
        self.i2c.smbus_write_byte(0x0f, crt1)?;

        // ctr2 - wrtc1
        let mut crt2 = self.i2c.smbus_read_byte(0x10)?;
        crt2 &= 0b0111_1111;
        self.i2c.smbus_write_byte(0x10, crt2)?;

        Ok(())
    }

    /// Disable frequency alarm
    pub fn disable_frequency_alarm(&self) -> Result<()> {
        self.enable_write()?;

        // CTR2 - INTS1=0, INTS0=1, INTFE=0
        let mut ctr2 = self.i2c.smbus_read_byte(0x10)?;
        ctr2 |= 0b0001_0000;
        ctr2 &= 0b1101_1110;
        self.i2c.smbus_write_byte(0x10, ctr2)?;

        self.disable_write()?;

        Ok(())
    }

    /// Set frequency alarm in auto_power_on, 1/2Hz
    pub fn enable_frequency_alarm(&self) -> Result<()> {
        self.enable_write()?;

        // CTR3 - 1/2Hz, FS3=1, FS2=0, FS1=1, FS0=1
        let mut ctr3 = self.i2c.smbus_read_byte(0x11)?;
        ctr3 |= 0b0000_1011;
        ctr3 &= 0b1111_1011;
        self.i2c.smbus_write_byte(0x11, ctr3)?;

        // CTR2 - INTS1=1, INTS0=0, INTFE=1, and disable INTAE, INTDE
        let mut ctr2 = self.i2c.smbus_read_byte(0x10)?;
        ctr2 |= 0b0010_0001;
        ctr2 &= 0b1110_1001;
        self.i2c.smbus_write_byte(0x10, ctr2)?;

        self.disable_write()?;

        Ok(())
    }

    pub fn enable_alarm(&self) -> Result<()> {
        self.enable_write()?;

        // CTR2 - alarm interrupt and frequency, INTS1=0, INTS0=1, INTDE=0, INTAE=1, INTFE=0
        let mut ctr2 = self.i2c.smbus_read_byte(0x10)?;
        ctr2 |= 0b0101_0010;
        ctr2 &= 0b1101_1010;
        self.i2c.smbus_write_byte(0x10, ctr2)?;

        // alarm allows weekday, hour/minus/second
        self.i2c.smbus_write_byte(0x0e, 0b0000_1111)?;

        self.disable_write()?;
        Ok(())
    }

    /// Disable alarm
    pub fn disable_alarm(&self) -> Result<()> {
        self.enable_write()?;

        // CTR2 - INTS1, clear
        let mut ctr2 = self.i2c.smbus_read_byte(0x10)?;
        ctr2 |= 0b0101_0010;
        ctr2 &= 0b1101_1111;
        self.i2c.smbus_write_byte(0x10, ctr2)?;

        // disable alarm
        self.i2c.smbus_write_byte(0x0e, 0b0000_0000)?;

        self.disable_write()?;

        Ok(())
    }

    /// Read battery charging flag
    pub fn read_battery_charging_flag(&self) -> Result<bool> {
        let v = self.i2c.smbus_read_byte(0x18)?;
        Ok(v & 0b1000_0000 != 0)
    }

    /// Check alarm enabled
    pub fn read_alarm_enabled(&self) -> Result<bool> {
        let v = self.i2c.smbus_read_byte(0x0e)?;
        if v & 0b0000_0111 == 0 {
            return Ok(false);
        }

        let ctr2 = self.i2c.smbus_read_byte(0x10)?;
        if ctr2 & 0b0000_0010 == 0 {
            return Ok(false);
        }

        Ok(true)
    }
}

impl RTC for SD3078 {
    /// Init
    fn init(&mut self, config: &PiSugarConfig) -> Result<()> {
        self.clear_alarm_flag()?;

        // NOTE enable frequency alarm
        if config.auto_power_on == Some(true) {
            self.enable_frequency_alarm()?;
        } else {
            self.disable_frequency_alarm()?;
            if let Some(auto_wakeup_time) = config.auto_wake_time.clone() {
                self.set_alarm(auto_wakeup_time.into(), config.auto_wake_repeat)?;
            }
        }

        Ok(())
    }

    /// Read time
    fn read_time(&self) -> Result<RTCRawTime> {
        let mut bcd_time = [0_u8; 7];
        self.i2c.block_read(0, &mut bcd_time)?;

        // 12hr or 24hr
        if bcd_time[2] & 0b1000_0000 != 0 {
            bcd_time[2] &= 0b0111_1111; // 24hr
        } else if bcd_time[2] & 0b0010_0000 != 0 {
            bcd_time[2] &= 0b0001_1111; // 12hr and pm
            let hour = bcd_to_dec(bcd_time[2]);
            let hour = hour + 12;
            bcd_time[2] = dec_to_bcd(hour);
        }

        Ok(RTCRawTime(bcd_time))
    }

    /// Write time
    fn write_time(&self, t: RTCRawTime) -> Result<()> {
        // 24h
        let mut bcd_time = t.0.clone();
        bcd_time[2] |= 0b1000_0000;

        self.enable_write()?;
        self.i2c.block_write(0, bcd_time.as_ref())?;
        self.disable_write()?;

        Ok(())
    }

    /// Read alarm time
    fn read_alarm_time(&self) -> Result<RTCRawTime> {
        let mut bcd_time = [0_u8; 7];
        self.i2c.block_read(0x07, &mut bcd_time)?;

        // always 24hr
        bcd_time[2] &= 0b0011_1111;

        bcd_time[4] = 1;
        bcd_time[5] = 1;
        bcd_time[6] = 0;

        Ok(RTCRawTime(bcd_time))
    }

    /// Set alarm, weekday_repeat from sunday 0-6
    fn set_alarm(&self, t: RTCRawTime, weekday_repeat: u8) -> Result<()> {
        let mut bcd_time = t.0.clone();
        bcd_time[3] = weekday_repeat;

        self.enable_write()?;

        // alarm time
        self.i2c.block_write(0x07, bcd_time.as_ref())?;

        // CTR2 - alarm interrupt and frequency, INTS1=0, INTS0=1, INTDE=0, INTAE=1, INTFE=0
        let mut ctr2 = self.i2c.smbus_read_byte(0x10)?;
        ctr2 |= 0b0101_0010;
        ctr2 &= 0b1101_1010;
        self.i2c.smbus_write_byte(0x10, ctr2)?;

        // alarm allows weekday, hour/minus/second
        self.i2c.smbus_write_byte(0x0e, 0b0000_1111)?;

        self.disable_write()?;

        Ok(())
    }

    fn is_alarm_enable(&self) -> Result<bool> {
        self.read_alarm_enabled()
    }

    fn toggle_alarm_enable(&self, enable: bool) -> Result<()> {
        if !enable {
            self.disable_alarm()
        } else {
            self.enable_alarm()
        }
    }

    /// Read alarm flag
    fn read_alarm_flag(&self) -> Result<bool> {
        // CTR1 - INTDF and INTAF
        let data = self.i2c.smbus_read_byte(0x0f)?;
        if data & 0b0010_0000 != 0 || data & 0b0001_0000 != 0 {
            return Ok(true);
        }

        Ok(false)
    }

    /// Clear alarm flag
    fn clear_alarm_flag(&self) -> Result<()> {
        if let Ok(true) = self.read_alarm_flag() {
            self.enable_write()?;
            let mut ctr1 = self.i2c.smbus_read_byte(0x0f)?;
            ctr1 &= 0b1100_1111;
            self.i2c.smbus_write_byte(0x0f, ctr1)?;

            self.disable_write()?;
        }
        Ok(())
    }

    fn toggle_frequency_alarm(&self, enable: bool) -> Result<()> {
        if !enable {
            self.disable_frequency_alarm()
        } else {
            self.enable_frequency_alarm()
        }
    }

    /// Force shutdown
    fn force_shutdown(&self) -> Result<()> {
        self.disable_frequency_alarm()
    }

    /// Read battery low flag
    fn read_battery_low_flag(&self) -> Result<bool> {
        let v = self.i2c.smbus_read_byte(0x1a)?;
        Ok(v & 0b0000_0001 != 0)
    }

    /// Toggle rtc battery charging
    fn toggle_charging(&self, enable: bool) -> Result<()> {
        self.enable_write()?;
        let v = if enable { 0x82 } else { 0x82 & 0b0111_1111 };
        self.i2c.smbus_write_byte(0x18, v)?;
        self.disable_write()
    }

    /// Read battery high flag
    fn read_battery_high_flag(&self) -> Result<bool> {
        let v = self.i2c.smbus_read_byte(0x1a)?;
        Ok(v & 0b0000_0010 != 0)
    }
}
