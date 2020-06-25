use std::convert::{TryFrom, TryInto};
use std::fmt::{self, Display};

use chrono::prelude::*;
use chrono::LocalResult;
use rppal::i2c::I2c;

use crate::Result;

/// SD3078 time, always UTC 24hr
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SD3078Time([u8; 7]);

impl SD3078Time {
    /// From raw sd3078 time
    pub fn from_raw(sd3078_raw: [u8; 7]) -> Self {
        Self(sd3078_raw)
    }

    /// From dec
    pub fn from_dec(dec: [u8; 7]) -> Self {
        let mut raw = [0; 7];
        for i in 0..7 {
            raw[i] = bcd_to_dec(dec[i]);
        }
        Self(raw)
    }

    /// Year, 2000-2099
    pub fn year(&self) -> u16 {
        bcd_to_dec(self.0[6]) as u16 + 2000
    }

    /// Month, 1-12
    pub fn month(&self) -> u8 {
        bcd_to_dec(self.0[5])
    }

    /// Day of month, 1-31
    pub fn day(&self) -> u8 {
        bcd_to_dec(self.0[4])
    }

    /// Weekday from sunday, 0-6
    pub fn weekday(&self) -> u8 {
        bcd_to_dec(self.0[3])
    }

    /// Hour, 0-23
    pub fn hour(&self) -> u8 {
        bcd_to_dec(self.0[2])
    }

    /// Minute, 0-59
    pub fn minute(&self) -> u8 {
        bcd_to_dec(self.0[1])
    }

    /// Second, 0-59
    pub fn second(&self) -> u8 {
        bcd_to_dec(self.0[0])
    }

    /// To dec
    pub fn to_dec(&self) -> [u8; 7] {
        [
            self.second(),
            self.minute(),
            self.hour(),
            self.weekday(),
            self.day(),
            self.month(),
            (self.year() - 2000) as u8,
        ]
    }
}

impl Display for SD3078Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{},{},{},{},{},{},{}]",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5], self.0[6]
        )
    }
}

impl From<DateTime<Utc>> for SD3078Time {
    fn from(dt: DateTime<Utc>) -> Self {
        let mut t = SD3078Time([0; 7]);
        t.0[6] = dec_to_bcd((dt.year() % 100) as u8);
        t.0[5] = dec_to_bcd(dt.month() as u8);
        t.0[4] = dec_to_bcd(dt.day() as u8);
        t.0[3] = dec_to_bcd(dt.weekday().num_days_from_sunday() as u8);
        t.0[2] = dec_to_bcd(dt.hour() as u8);
        t.0[1] = dec_to_bcd(dt.minute() as u8);
        t.0[0] = dec_to_bcd(dt.second() as u8);
        t
    }
}

impl TryFrom<SD3078Time> for DateTime<Utc> {
    type Error = ();

    fn try_from(t: SD3078Time) -> std::result::Result<Self, Self::Error> {
        let sec = bcd_to_dec(t.0[0]) as u32;
        let min = bcd_to_dec(t.0[1]) as u32;
        let hour = bcd_to_dec(t.0[2]) as u32;
        let day_of_month = bcd_to_dec(t.0[4]) as u32;
        let month = bcd_to_dec(t.0[5]) as u32;
        let year = 2000 + bcd_to_dec(t.0[6]) as i32;

        let datetime = Utc
            .ymd_opt(year, month, day_of_month)
            .and_hms_opt(hour, min, sec);
        match datetime {
            LocalResult::Single(datetime) => Ok(datetime),
            _ => Err(()),
        }
    }
}

impl From<DateTime<Local>> for SD3078Time {
    fn from(dt: DateTime<Local>) -> Self {
        let dt: DateTime<Utc> = DateTime::from(dt);
        dt.into()
    }
}

impl TryFrom<SD3078Time> for DateTime<Local> {
    type Error = ();

    fn try_from(t: SD3078Time) -> std::result::Result<Self, Self::Error> {
        t.try_into()
            .and_then(|dt: DateTime<Utc>| Ok(DateTime::from(dt)))
    }
}

/// SD3078, rtc chip
pub struct SD3078 {
    i2c: I2c,
}

impl SD3078 {
    /// Create new SD3078
    pub fn new(i2c_addr: u16) -> Result<Self> {
        let mut i2c = I2c::new()?;
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

    /// Read battery low flag
    pub fn read_battery_low_flag(&self) -> Result<bool> {
        let v = self.i2c.smbus_read_byte(0x1a)?;
        Ok(v & 0b0000_0001 != 0)
    }

    /// Read battery high flag
    pub fn read_battery_high_flag(&self) -> Result<bool> {
        let v = self.i2c.smbus_read_byte(0x1a)?;
        Ok(v & 0b0000_0010 != 0)
    }

    /// Read battery charging flag
    pub fn read_battery_charging_flag(&self) -> Result<bool> {
        let v = self.i2c.smbus_read_byte(0x18)?;
        Ok(v & 0b1000_0000 != 0)
    }

    /// Toggle rtc battery charging
    pub fn toggle_charging(&self, enable: bool) -> Result<()> {
        self.enable_write()?;
        let v = if enable { 0x82 } else { 0x82 & 0b0111_1111 };
        self.i2c.smbus_write_byte(0x18, v)?;
        self.disable_write()
    }

    /// Read time
    pub fn read_time(&self) -> Result<SD3078Time> {
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

        Ok(SD3078Time(bcd_time))
    }

    /// Write time
    pub fn write_time(&self, t: SD3078Time) -> Result<()> {
        // 24h
        let mut bcd_time = t.0.clone();
        bcd_time[2] |= 0b1000_0000;

        self.enable_write()?;
        self.i2c.block_write(0, bcd_time.as_ref())?;
        self.disable_write()?;

        Ok(())
    }

    /// Read alarm time
    pub fn read_alarm_time(&self) -> Result<SD3078Time> {
        let mut bcd_time = [0_u8; 7];
        self.i2c.block_read(0x07, &mut bcd_time)?;

        // always 24hr
        bcd_time[2] &= 0b0011_1111;

        Ok(SD3078Time(bcd_time))
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

    /// Read alarm flag
    pub fn read_alarm_flag(&self) -> Result<bool> {
        // CTR1 - INTDF and INTAF
        let data = self.i2c.smbus_read_byte(0x0f)?;
        if data & 0b0010_0000 != 0 || data & 0b0001_0000 != 0 {
            return Ok(true);
        }

        Ok(false)
    }

    /// Clear alarm flag
    pub fn clear_alarm_flag(&self) -> Result<()> {
        if let Ok(true) = self.read_alarm_flag() {
            self.enable_write()?;
            let mut ctr1 = self.i2c.smbus_read_byte(0x0f)?;
            ctr1 &= 0b1100_1111;
            self.i2c.smbus_write_byte(0x0f, ctr1)?;

            self.disable_write()?;
        }
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

    /// Set alarm, weekday_repeat from sunday 0-6
    pub fn set_alarm(&self, t: SD3078Time, weekday_repeat: u8) -> Result<()> {
        let mut bcd_time = t.0.clone();
        bcd_time[3] = weekday_repeat;

        self.enable_write()?;

        // alarm time
        self.i2c.block_write(0x07, bcd_time.as_ref())?;

        // CTR2 - alarm interrupt and frequency
        let mut ctr2 = self.i2c.smbus_read_byte(0x10)?;
        ctr2 |= 0b0101_0010;
        ctr2 &= 0b1101_1111;
        self.i2c.smbus_write_byte(0x10, ctr2)?;

        // alarm allows weekday, hour/minus/second
        self.i2c.smbus_write_byte(0x0e, 0b0000_1111)?;

        self.disable_write()?;

        Ok(())
    }

    /// Set a test wake up after 1 minutes
    pub fn set_test_wake(&self) -> Result<()> {
        let now = Utc::now();
        self.write_time(now.into())?;

        let duration = chrono::Duration::seconds(90);
        let then = now + duration;
        self.set_alarm(then.into(), 0b0111_1111)?;

        log::error!("Will wake up after 1min 30sec, please power-off");

        Ok(())
    }
}

fn bcd_to_dec(bcd: u8) -> u8 {
    (bcd & 0x0F) + (((bcd & 0xF0) >> 4) * 10)
}

fn dec_to_bcd(dec: u8) -> u8 {
    dec % 10 + ((dec / 10) << 4)
}
