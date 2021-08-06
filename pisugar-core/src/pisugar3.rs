use std::collections::VecDeque;
use std::time::Instant;

use chrono::{DateTime, Duration, Local, Timelike};
use rppal::i2c::I2c;

use crate::battery::Battery;
use crate::ip5312::IP5312;
use crate::rtc::{bcd_to_dec, dec_to_bcd, RTC};
use crate::{Error, Model, RTCRawTime, Result, TapType};

/// PiSugar 3 i2c addr
pub const I2C_ADDR_P3: u16 = 0x57;

/// Global ctrl 1
const IIC_CMD_CTR1: u8 = 0x02;

/// Global ctrl 2
const IIC_CMD_CTR2: u8 = 0x03;

/// Tap
const IIC_CMD_TAP: u8 = 0x08;

/// Battery ctrl
const IIC_CMD_BAT_CTR: u8 = 0x20;

/// Voltage high byte
const IIC_CMD_VH: u8 = 0x22;
/// Voltage low byte
const IIC_CMD_VL: u8 = 0x23;

/// Output current high byte
const IIC_CMD_OH: u8 = 0x26;
/// Output current lob byte
const IIC_CMD_OL: u8 = 0x27;

const IIC_CMD_P: u8 = 0x2A;

/// RTC year
const IIC_CMD_RTC_YY: u8 = 0x31;
/// RTC month
const IIC_CMD_RTC_MM: u8 = 0x32;
/// RTC day of month
const IIC_CMD_RTC_DD: u8 = 0x33;
/// RTC weekday
const IIC_CMD_RTC_WD: u8 = 0x34;
/// RTC hour
const IIC_CMD_RTC_HH: u8 = 0x35;
/// RTC minute
const IIC_CMD_RTC_MN: u8 = 0x36;
/// RTC second
const IIC_CMD_RTC_SS: u8 = 0x37;

/// Alarm ctrl
const IIC_CMD_ALM_CTR: u8 = 0x40;
/// Alarm weekday repeat
const IIC_CMD_ALM_WD: u8 = 0x44;
/// Alarm hour
const IIC_CMD_ALM_HH: u8 = 0x45;
/// Alarm minute
const IIC_CMD_ALM_MN: u8 = 0x46;
/// Alarm second
const IIC_CMD_ALM_SS: u8 = 0x47;

/// PiSugar 3
pub struct PiSugar3 {
    i2c: I2c,
}

impl PiSugar3 {
    pub fn new(i2c_bus: u8, i2c_addr: u16) -> Result<Self> {
        let mut i2c = I2c::with_bus(i2c_bus)?;
        i2c.set_slave_address(i2c_addr)?;
        Ok(Self { i2c })
    }

    pub fn read_ctr1(&self) -> Result<u8> {
        let ctr1 = self.i2c.smbus_read_byte(IIC_CMD_CTR1)?;
        Ok(ctr1)
    }

    pub fn write_ctr1(&self, ctr1: u8) -> Result<()> {
        self.i2c.smbus_write_byte(IIC_CMD_CTR1, ctr1)?;
        Ok(())
    }

    pub fn read_crt2(&self) -> Result<u8> {
        let ctr2 = self.i2c.smbus_read_byte(IIC_CMD_CTR2)?;
        Ok(ctr2)
    }

    pub fn write_ctr2(&self, ctr2: u8) -> Result<()> {
        self.i2c.smbus_write_byte(IIC_CMD_CTR2, ctr2)?;
        Ok(())
    }

    pub fn toggle_restore(&self, auto_restore: bool) -> Result<()> {
        let mut ctr1 = self.read_ctr1()?;
        ctr1 &= 0b1110_0000;
        if auto_restore {
            ctr1 |= 0b0001_0000;
        }
        self.write_ctr1(ctr1)
    }

    pub fn read_tap(&self) -> Result<u8> {
        let tap = self.i2c.smbus_read_byte(IIC_CMD_TAP)?;
        Ok(tap & 0b0000_0011)
    }

    pub fn reset_tap(&self) -> Result<()> {
        let tap = self.i2c.smbus_read_byte(IIC_CMD_TAP)?;
        self.i2c.smbus_write_byte(IIC_CMD_TAP, tap & 0b1111_1100)?;
        Ok(())
    }

    pub fn read_bat_ctr(&self) -> Result<u8> {
        let ctr = self.i2c.smbus_read_byte(IIC_CMD_BAT_CTR)?;
        Ok(ctr)
    }

    pub fn write_bat_ctr(&self, ctr: u8) -> Result<()> {
        self.i2c.smbus_write_byte(IIC_CMD_BAT_CTR, ctr)?;
        Ok(())
    }

    pub fn read_bat_input_protected(&self) -> Result<bool> {
        let ctr = self.read_bat_ctr()?;
        Ok((ctr & 1 << 7) != 0)
    }

    pub fn toggle_bat_input_protected(&self, enable: bool) -> Result<()> {
        let mut ctr = self.read_bat_ctr()?;
        ctr &= 0b0111_1111;
        if enable {
            ctr |= 1 << 7;
        }
        self.write_bat_ctr(ctr)?;
        Ok(())
    }

    pub fn read_voltage(&self) -> Result<u16> {
        let vh: u16 = self.i2c.smbus_read_byte(IIC_CMD_VH)? as u16;
        let vl: u16 = self.i2c.smbus_read_byte(IIC_CMD_VL)? as u16;
        let v = (vh << 8) | vl;
        Ok(v)
    }

    pub fn read_percent(&self) -> Result<u8> {
        let p = self.i2c.smbus_read_byte(IIC_CMD_P)?;
        Ok(p)
    }

    pub fn read_output_current(&self) -> Result<u16> {
        let oh: u16 = self.i2c.smbus_read_byte(IIC_CMD_OH)? as u16;
        let ol: u16 = self.i2c.smbus_read_byte(IIC_CMD_OL)? as u16;
        let oc = (oh << 8) | ol;
        Ok(oc)
    }

    pub fn get_alarm_enable(&self) -> Result<bool> {
        let ctr = self.i2c.smbus_read_byte(IIC_CMD_ALM_CTR)?;
        Ok(ctr & (0b1000_0000) != 0)
    }

    pub fn toggle_alarm_enable(&self, enable: bool) -> Result<()> {
        let mut ctr = self.i2c.smbus_read_byte(IIC_CMD_ALM_CTR)?;
        ctr &= 0b0111_1111;
        if enable {
            ctr |= 0b1000_0000;
        }
        self.i2c.smbus_write_byte(IIC_CMD_ALM_CTR, ctr)?;
        Ok(())
    }

    pub fn get_rtc_yy(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c.smbus_read_byte(IIC_CMD_RTC_YY)?))
    }

    pub fn set_rtc_yy(&self, yy: u8) -> Result<()> {
        Ok(self.i2c.smbus_write_byte(IIC_CMD_RTC_YY, dec_to_bcd(yy))?)
    }

    pub fn get_rtc_mm(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c.smbus_read_byte(IIC_CMD_RTC_MM)?))
    }

    pub fn set_rtc_mm(&self, mm: u8) -> Result<()> {
        Ok(self.i2c.smbus_write_byte(IIC_CMD_RTC_MM, dec_to_bcd(mm))?)
    }

    pub fn get_rtc_dd(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c.smbus_read_byte(IIC_CMD_RTC_DD)?))
    }

    pub fn set_rtc_dd(&self, dd: u8) -> Result<()> {
        Ok(self.i2c.smbus_write_byte(IIC_CMD_RTC_DD, dec_to_bcd(dd))?)
    }

    pub fn get_rtc_weekday(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c.smbus_read_byte(IIC_CMD_RTC_WD)?))
    }

    pub fn set_rtc_weekday(&self, wd: u8) -> Result<()> {
        Ok(self.i2c.smbus_write_byte(IIC_CMD_RTC_WD, dec_to_bcd(wd))?)
    }

    pub fn get_rtc_hh(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c.smbus_read_byte(IIC_CMD_RTC_HH)?))
    }

    pub fn set_rtc_hh(&self, hh: u8) -> Result<()> {
        Ok(self.i2c.smbus_write_byte(IIC_CMD_RTC_HH, dec_to_bcd(hh))?)
    }

    pub fn get_rtc_mn(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c.smbus_read_byte(IIC_CMD_RTC_MN)?))
    }

    pub fn set_rtc_mn(&self, mn: u8) -> Result<()> {
        Ok(self.i2c.smbus_write_byte(IIC_CMD_RTC_MN, dec_to_bcd(mn))?)
    }

    pub fn get_rtc_ss(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c.smbus_read_byte(IIC_CMD_RTC_SS)?))
    }

    pub fn set_rtc_ss(&self, ss: u8) -> Result<()> {
        Ok(self.i2c.smbus_write_byte(IIC_CMD_RTC_SS, dec_to_bcd(ss))?)
    }

    pub fn get_alarm_weekday_repeat(&self) -> Result<u8> {
        Ok(self.i2c.smbus_read_byte(IIC_CMD_ALM_WD)?)
    }

    pub fn set_alarm_weekday_repeat(&self, wd: u8) -> Result<()> {
        Ok(self.i2c.smbus_write_byte(IIC_CMD_ALM_WD, wd)?)
    }

    pub fn get_alarm_hh(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c.smbus_read_byte(IIC_CMD_ALM_HH)?))
    }

    pub fn set_alarm_hh(&self, hh: u8) -> Result<()> {
        Ok(self.i2c.smbus_write_byte(IIC_CMD_ALM_HH, dec_to_bcd(hh))?)
    }

    pub fn get_alarm_mn(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c.smbus_read_byte(IIC_CMD_ALM_MN)?))
    }

    pub fn set_alarm_mn(&self, mn: u8) -> Result<()> {
        Ok(self.i2c.smbus_write_byte(IIC_CMD_ALM_MN, dec_to_bcd(mn))?)
    }

    pub fn get_alarm_ss(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c.smbus_read_byte(IIC_CMD_ALM_SS)?))
    }

    pub fn set_alarm_ss(&self, ss: u8) -> Result<()> {
        Ok(self.i2c.smbus_write_byte(IIC_CMD_ALM_SS, dec_to_bcd(ss))?)
    }
}

/// PiSugar 3 Battery support
pub struct PiSugar3Battery {
    pisugar3: PiSugar3,
    model: Model,
    voltages: VecDeque<(Instant, f32)>,
    intensities: VecDeque<(Instant, f32)>,
    levels: VecDeque<f32>,
    poll_at: Instant,
}

impl PiSugar3Battery {
    pub fn new(i2c_bus: u8, i2c_addr: u16, model: Model) -> Result<Self> {
        let pisugar3 = PiSugar3::new(i2c_bus, i2c_addr)?;
        let poll_at = Instant::now() - std::time::Duration::from_secs(10);
        Ok(Self {
            pisugar3,
            model,
            voltages: VecDeque::with_capacity(30),
            intensities: VecDeque::with_capacity(30),
            levels: VecDeque::with_capacity(30),
            poll_at,
        })
    }
}

impl Battery for PiSugar3Battery {
    fn init(&mut self, auto_power_on: bool) -> crate::Result<()> {
        Ok(self.pisugar3.toggle_restore(auto_power_on)?)
    }

    fn model(&self) -> String {
        self.model.to_string()
    }

    fn led_amount(&self) -> crate::Result<u32> {
        Ok(self.model.led_amount())
    }

    fn voltage(&self) -> crate::Result<f32> {
        let v = self.pisugar3.read_voltage()?;
        Ok((v as f32) / 1000.0)
    }

    fn voltage_avg(&self) -> crate::Result<f32> {
        let mut total = 0.0;
        self.voltages.iter().for_each(|v| total += v.1);
        if self.voltages.len() > 0 {
            Ok(total / self.voltages.len() as f32)
        } else {
            Err(Error::Other("Require initialization".to_string()))
        }
    }

    fn level(&self) -> crate::Result<f32> {
        self.voltage_avg().and_then(|v| Ok(IP5312::parse_voltage_level(v)))
    }

    fn intensity(&self) -> crate::Result<f32> {
        let c = self.pisugar3.read_output_current()?;
        Ok((c as f32) / 1000 as f32)
    }

    fn intensity_avg(&self) -> crate::Result<f32> {
        let mut total = 0.0;
        self.intensities.iter().for_each(|i| total += i.1);
        if self.intensities.len() > 0 {
            Ok(total / self.intensities.len() as f32)
        } else {
            Err(Error::Other("Require initialization".to_string()))
        }
    }

    fn is_power_plugged(&self) -> crate::Result<bool> {
        let ctr1 = self.pisugar3.read_ctr1()?;
        Ok((ctr1 & (1 << 7)) != 0)
    }

    fn is_allow_charging(&self) -> crate::Result<bool> {
        let ctr1 = self.pisugar3.read_ctr1()?;
        Ok((ctr1 & (1 << 6)) != 0)
    }

    fn toggle_allow_charging(&self, enable: bool) -> crate::Result<()> {
        let mut ctr1 = self.pisugar3.read_ctr1()?;
        ctr1 &= 0b1011_1111;
        if enable {
            ctr1 |= 0b0100_0000;
        }
        self.pisugar3.write_ctr1(ctr1)
    }

    fn is_charging(&self) -> crate::Result<bool> {
        let power_plugged = self.is_power_plugged()?;
        let allow_charging = self.is_allow_charging()?;
        return Ok(power_plugged && allow_charging);
    }

    fn is_input_protected(&self) -> Result<bool> {
        self.pisugar3.read_bat_input_protected()
    }

    fn toggle_input_protected(&self, enable: bool) -> Result<()> {
        self.pisugar3.toggle_bat_input_protected(enable)
    }

    fn poll(&mut self, now: Instant) -> crate::Result<Option<TapType>> {
        // slow down, 500ms
        if self.poll_at > now || self.poll_at + std::time::Duration::from_millis(500) > now {
            return Ok(None);
        }
        self.poll_at = now;

        let voltage = self.voltage()?;
        self.voltages.pop_front();
        while self.voltages.len() < self.voltages.capacity() {
            self.voltages.push_back((now, voltage));
        }

        let level = IP5312::parse_voltage_level(voltage);
        self.levels.pop_front();
        while self.levels.len() < self.levels.capacity() {
            self.levels.push_back(level);
        }

        let intensity = self.intensity()?;
        self.intensities.pop_front();
        while self.intensities.len() < self.intensities.capacity() {
            self.intensities.push_back((now, intensity));
        }

        let tap = match self.pisugar3.read_tap()? {
            1 => Some(TapType::Single),
            2 => Some(TapType::Double),
            3 => Some(TapType::Long),
            _ => None,
        };
        if tap.is_some() {
            self.pisugar3.reset_tap()?;
        }

        Ok(tap)
    }

    fn shutdown(&self) -> crate::Result<()> {
        let mut ctr1 = self.pisugar3.read_ctr1()?;
        ctr1 &= 0b1101_1111;
        self.pisugar3.write_ctr1(ctr1)
    }

    fn toggle_light_load_shutdown(&self, enable: bool) -> crate::Result<()> {
        let mut bat_ctr = self.pisugar3.read_bat_ctr()?;
        bat_ctr &= 0b1101_1111;
        if enable {
            bat_ctr |= 0b0010_0000;
        }
        self.pisugar3.write_bat_ctr(bat_ctr)?;
        Ok(())
    }
}

pub struct PiSugar3RTC {
    pisugar3: PiSugar3,
}

impl PiSugar3RTC {
    pub fn new(i2c_bus: u8, i2c_addr: u16) -> Result<Self> {
        let pisugar3 = PiSugar3::new(i2c_bus, i2c_addr)?;
        Ok(Self { pisugar3 })
    }
}

impl RTC for PiSugar3RTC {
    fn init(&self, auto_power_on: bool, auto_wakeup_time: Option<DateTime<Local>>, wakeup_repeat: u8) -> Result<()> {
        self.pisugar3.toggle_restore(auto_power_on)?;
        if let Some(wakeup_time) = auto_wakeup_time {
            self.pisugar3.toggle_alarm_enable(false)?;
            self.pisugar3.set_alarm_hh(wakeup_time.hour() as u8)?;
            self.pisugar3.set_alarm_mn(wakeup_time.minute() as u8)?;
            self.pisugar3.set_alarm_ss(wakeup_time.second() as u8)?;
            self.pisugar3.set_alarm_weekday_repeat(wakeup_repeat)?;
            self.pisugar3.toggle_alarm_enable(true)?;
        }
        Ok(())
    }

    fn read_time(&self) -> Result<RTCRawTime> {
        Ok(RTCRawTime::from_dec([
            self.pisugar3.get_rtc_ss()?,
            self.pisugar3.get_rtc_mn()?,
            self.pisugar3.get_rtc_hh()?,
            self.pisugar3.get_rtc_weekday()?,
            self.pisugar3.get_rtc_dd()?,
            self.pisugar3.get_rtc_mm()?,
            self.pisugar3.get_rtc_yy()?,
        ]))
    }

    fn write_time(&self, raw: RTCRawTime) -> Result<()> {
        self.pisugar3.set_rtc_ss(raw.second())?;
        self.pisugar3.set_rtc_mn(raw.minute())?;
        self.pisugar3.set_rtc_hh(raw.hour())?;
        self.pisugar3.set_rtc_weekday(raw.weekday())?;
        self.pisugar3.set_rtc_dd(raw.day())?;
        self.pisugar3.set_rtc_mm(raw.month())?;
        self.pisugar3.set_rtc_yy(((raw.year() - 2000) & 0xff) as u8)?;
        Ok(())
    }

    fn read_alarm_time(&self) -> Result<RTCRawTime> {
        let mut raw = RTCRawTime::from_dec([
            self.pisugar3.get_alarm_ss()?,
            self.pisugar3.get_alarm_mn()?,
            self.pisugar3.get_alarm_hh()?,
            0,
            0,
            0,
            0,
        ]);
        raw.0[3] = self.pisugar3.get_alarm_weekday_repeat()?;
        Ok(raw)
    }

    fn set_alarm(&self, time: RTCRawTime, weekday_repeat: u8) -> Result<()> {
        self.pisugar3.toggle_alarm_enable(false)?;
        self.pisugar3.set_alarm_hh(time.hour())?;
        self.pisugar3.set_alarm_mn(time.minute())?;
        self.pisugar3.set_alarm_ss(time.second())?;
        self.pisugar3.set_alarm_weekday_repeat(weekday_repeat)?;
        self.pisugar3.toggle_alarm_enable(true)?;
        Ok(())
    }

    fn is_alarm_enable(&self) -> Result<bool> {
        Ok(self.pisugar3.get_alarm_enable()?)
    }

    fn toggle_alarm_enable(&self, enable: bool) -> Result<()> {
        Ok(self.pisugar3.toggle_alarm_enable(enable)?)
    }

    fn read_alarm_flag(&self) -> Result<bool> {
        // PiSugar 3 has no alarm flag
        Ok(false)
    }

    fn clear_alarm_flag(&self) -> Result<()> {
        Ok(())
    }

    fn toggle_frequency_alarm(&self, _enable: bool) -> Result<()> {
        // PiSugar 3 has auto power restore, so frequency alarm is deprecated
        Ok(())
    }

    fn force_shutdown(&self) -> Result<()> {
        Ok(())
    }

    fn read_battery_low_flag(&self) -> Result<bool> {
        Ok(false)
    }

    fn toggle_charging(&self, _enable: bool) -> Result<()> {
        Ok(())
    }

    fn read_battery_high_flag(&self) -> Result<bool> {
        Ok(true)
    }
}
