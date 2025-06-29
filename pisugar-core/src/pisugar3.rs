#![allow(dead_code)]

use std::collections::VecDeque;
use std::ffi::CStr;
use std::time::Instant;

use rppal::i2c::I2c;

use crate::ip5312::IP5312;
use crate::rtc::{bcd_to_dec, dec_to_bcd, RTC};
use crate::{
    battery::{Battery, BatteryEvent},
    ip5312::BATTERY_CURVE,
};
use crate::{Error, Model, PiSugarConfig, RTCRawTime, Result, TapType};

/// PiSugar 3 i2c addr
pub const I2C_ADDR_P3: u16 = 0x57;

/// Global ctrl 1
const IIC_CMD_CTR1: u8 = 0x02;

/// Global ctrl 2
const IIC_CMD_CTR2: u8 = 0x03;

/// Temperature
const IIC_CMD_TEMP: u8 = 0x04;

/// Tap
const IIC_CMD_TAP: u8 = 0x08;

/// PiSugar 3 write protect
const IIC_CMD_WRITE_ENABLE: u8 = 0x0B;

/// Battery ctrl
const IIC_CMD_BAT_CTR: u8 = 0x20;
const IIC_CMD_BAT_CTR2: u8 = 0x21;

/// Voltage high byte
const IIC_CMD_VH: u8 = 0x22;
/// Voltage low byte
const IIC_CMD_VL: u8 = 0x23;

/// Output current high byte
const IIC_CMD_OH: u8 = 0x26;
/// Output current lob byte
const IIC_CMD_OL: u8 = 0x27;

const IIC_CMD_P: u8 = 0x2A;

/// RTC Ctrl
const IIC_CMD_RTC_CTRL: u8 = 0x30;
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
/// RTC adjust 32s common(every second), 1bit direction, 4bit value
const IIC_CMD_RTC_ADJ_COMM: u8 = 0x3A;
/// RTC adjust 32s diff(only in 31s), 5bit value
const IIC_CMD_RTC_ADJ_DIFF: u8 = 0x3B;

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

// RTC I2C address
const IIC_CMD_RTC_ADDR: u8 = 0x51;

/// Firmware version
const IIC_CMD_APPVER: u8 = 0xE2;
const APP_VER_LEN: usize = 15;

/// PiSugar 3
pub struct PiSugar3 {
    i2c: I2c,
}

impl PiSugar3 {
    pub fn new(i2c_bus: u8, i2c_addr: u16) -> Result<Self> {
        log::debug!("PiSugar3 bus 0x{:02x} addr 0x{:02x}", i2c_bus, i2c_addr);
        let mut i2c = I2c::with_bus(i2c_bus)?;
        i2c.set_slave_address(i2c_addr)?;
        Ok(Self { i2c })
    }

    fn i2c_write_byte(&self, cmd: u8, data: u8) -> Result<()> {
        log::debug!("i2c write cmd: 0x{:02x}, data: 0x{:02x}", cmd, data);
        self.toggle_write_enable(true)?;
        let r = self.i2c.smbus_write_byte(cmd, data);
        self.toggle_write_enable(false)?;
        Ok(r?)
    }

    fn i2c_read_byte(&self, cmd: u8) -> Result<u8> {
        log::debug!("i2c read cmd: 0x{:02x}", cmd);
        let r = self.i2c.smbus_read_byte(cmd)?;
        log::debug!("i2c read cmd: 0x{:02x}, data: 0x{:02x}", cmd, r);
        Ok(r)
    }

    pub fn read_ctr1(&self) -> Result<u8> {
        let ctr1 = self.i2c_read_byte(IIC_CMD_CTR1)?;
        let ctr1_again = self.i2c_read_byte(IIC_CMD_CTR1)?;
        if ctr1 != ctr1_again {
            log::warn!(
                "ctr1 0x{:02x} changed during reading 0x{:02x}!=0x{:02x}",
                IIC_CMD_CTR1,
                ctr1,
                ctr1_again
            );
        }
        Ok(ctr1)
    }

    pub fn write_ctr1(&self, ctr1: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_CTR1, ctr1)?;
        Ok(())
    }

    pub fn read_crt2(&self) -> Result<u8> {
        let ctr2 = self.i2c_read_byte(IIC_CMD_CTR2)?;
        let ctr2_again = self.i2c_read_byte(IIC_CMD_CTR2)?;
        if ctr2 != ctr2_again {
            log::warn!(
                "ctr2 0x{:02x} changed during reading 0x{:x}!=0x{:x}",
                IIC_CMD_CTR2,
                ctr2,
                ctr2_again
            );
        }
        Ok(ctr2)
    }

    pub fn write_ctr2(&self, ctr2: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_CTR2, ctr2)?;
        Ok(())
    }

    pub fn read_output_enabled(&self) -> Result<bool> {
        let ctr1 = self.read_ctr1()?;
        Ok(ctr1 & (1 << 5) != 0)
    }

    pub fn toggle_output_enabled(&self, enable: bool) -> Result<()> {
        let mut ctr1 = self.read_ctr1()?;
        ctr1 &= 0b1101_1111;
        if enable {
            ctr1 |= 1 << 5;
        }
        self.write_ctr1(ctr1)?;
        Ok(())
    }

    pub fn toggle_restore(&self, auto_restore: bool) -> Result<()> {
        let mut ctr1 = self.read_ctr1()?;
        ctr1 &= 0b1110_1111;
        if auto_restore {
            ctr1 |= 0b0001_0000;
        }
        self.write_ctr1(ctr1)
    }

    pub fn toggle_soft_poweroff(&self, enable: bool) -> Result<()> {
        let mut ctr2 = self.read_crt2()?;
        ctr2 &= 0b1110_0000;
        if enable {
            ctr2 |= 0b0001_0000;
        }
        self.write_ctr2(ctr2)
    }

    pub fn read_soft_poweroff_flag(&self) -> Result<bool> {
        let ctr2 = self.read_crt2()?;
        // soft poweroff, bit4 and bit3 must be 1 at the same time
        Ok((ctr2 & 0b0001_1000) == 0b0001_1000)
    }

    pub fn clear_soft_poweroff_flag(&self) -> Result<()> {
        let mut ctr2 = self.read_crt2()?;
        ctr2 &= 0b1111_0111;
        self.write_ctr2(ctr2)
    }

    pub fn read_temp(&self) -> Result<i32> {
        let temp = self.i2c_read_byte(IIC_CMD_TEMP)?;
        Ok(temp as i32 - 40)
    }

    pub fn read_tap(&self) -> Result<u8> {
        let tap = self.i2c_read_byte(IIC_CMD_TAP)?;
        Ok(tap & 0b0000_0011)
    }

    pub fn reset_tap(&self) -> Result<()> {
        let tap = self.i2c_read_byte(IIC_CMD_TAP)?;
        self.i2c_write_byte(IIC_CMD_TAP, tap & 0b1111_1100)?;
        Ok(())
    }

    pub fn read_bat_ctr(&self) -> Result<u8> {
        let ctr = self.i2c_read_byte(IIC_CMD_BAT_CTR)?;
        Ok(ctr)
    }

    pub fn write_bat_ctr(&self, ctr: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_BAT_CTR, ctr)?;
        Ok(())
    }

    pub fn read_bat_ctr2(&self) -> Result<u8> {
        let ctr = self.i2c_read_byte(IIC_CMD_BAT_CTR2)?;
        Ok(ctr)
    }

    pub fn write_bat_ctr2(&self, ctr: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_BAT_CTR2, ctr)?;
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

    pub fn read_write_enable(&self) -> Result<bool> {
        let ctr = self.i2c_read_byte(IIC_CMD_WRITE_ENABLE)?;
        Ok(ctr == 0x29)
    }

    pub fn toggle_write_enable(&self, enable: bool) -> Result<()> {
        let ctr = if enable { 0x29 } else { 0b0000_0000 };
        self.i2c.smbus_write_byte(IIC_CMD_WRITE_ENABLE, ctr)?;
        Ok(())
    }

    pub fn read_voltage(&self) -> Result<u16> {
        let vh: u16 = self.i2c_read_byte(IIC_CMD_VH)? as u16;
        let vl: u16 = self.i2c_read_byte(IIC_CMD_VL)? as u16;
        let v = (vh << 8) | vl;
        Ok(v)
    }

    pub fn read_percent(&self) -> Result<u8> {
        let p = self.i2c_read_byte(IIC_CMD_P)?;
        Ok(p)
    }

    pub fn read_output_current(&self) -> Result<u16> {
        let oh: u16 = self.i2c_read_byte(IIC_CMD_OH)? as u16;
        let ol: u16 = self.i2c_read_byte(IIC_CMD_OL)? as u16;
        let oc = (oh << 8) | ol;
        Ok(oc)
    }

    pub fn get_alarm_enable(&self) -> Result<bool> {
        let ctr = self.i2c_read_byte(IIC_CMD_ALM_CTR)?;
        Ok(ctr & (0b1000_0000) != 0)
    }

    pub fn toggle_alarm_enable(&self, enable: bool) -> Result<()> {
        let mut ctr = self.i2c_read_byte(IIC_CMD_ALM_CTR)?;
        ctr &= 0b0111_1111;
        if enable {
            ctr |= 0b1000_0000;
        }
        self.i2c_write_byte(IIC_CMD_ALM_CTR, ctr)?;
        Ok(())
    }

    pub fn read_rtc_yy(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c_read_byte(IIC_CMD_RTC_YY)?))
    }

    fn write_rtc_yy(&self, yy: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_RTC_YY, dec_to_bcd(yy))
    }

    pub fn read_rtc_mm(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c_read_byte(IIC_CMD_RTC_MM)?))
    }

    fn write_rtc_mm(&self, mm: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_RTC_MM, dec_to_bcd(mm))
    }

    pub fn read_rtc_dd(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c_read_byte(IIC_CMD_RTC_DD)?))
    }

    fn write_rtc_dd(&self, dd: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_RTC_DD, dec_to_bcd(dd))
    }

    pub fn read_rtc_weekday(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c_read_byte(IIC_CMD_RTC_WD)?))
    }

    fn write_rtc_weekday(&self, wd: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_RTC_WD, dec_to_bcd(wd))
    }

    pub fn read_rtc_hh(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c_read_byte(IIC_CMD_RTC_HH)?))
    }

    fn write_rtc_hh(&self, hh: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_RTC_HH, dec_to_bcd(hh))
    }

    pub fn read_rtc_mn(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c_read_byte(IIC_CMD_RTC_MN)?))
    }

    fn write_rtc_mn(&self, mn: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_RTC_MN, dec_to_bcd(mn))
    }

    pub fn read_rtc_ss(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c_read_byte(IIC_CMD_RTC_SS)?))
    }

    fn write_rtc_ss(&self, ss: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_RTC_SS, dec_to_bcd(ss))
    }

    pub fn read_rtc_adj_comm(&self) -> Result<u8> {
        self.i2c_read_byte(IIC_CMD_RTC_ADJ_COMM)
    }

    pub fn write_rtc_adj_comm(&self, comm: u8) -> Result<()> {
        let comm = comm & 0b1000_1111;
        self.i2c_write_byte(IIC_CMD_RTC_ADJ_COMM, comm)
    }

    pub fn read_rtc_adj_diff(&self) -> Result<u8> {
        self.i2c_read_byte(IIC_CMD_RTC_ADJ_DIFF)
    }

    pub fn write_rtc_adj_diff(&self, diff: u8) -> Result<()> {
        let diff = diff & 0b0001_1111;
        self.i2c_write_byte(IIC_CMD_RTC_ADJ_DIFF, diff)
    }

    pub fn read_alarm_weekday_repeat(&self) -> Result<u8> {
        self.i2c_read_byte(IIC_CMD_ALM_WD)
    }

    fn write_alarm_weekday_repeat(&self, wd: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_ALM_WD, wd)
    }

    pub fn read_alarm_hh(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c_read_byte(IIC_CMD_ALM_HH)?))
    }

    fn write_alarm_hh(&self, hh: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_ALM_HH, dec_to_bcd(hh))
    }

    pub fn read_alarm_mn(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c_read_byte(IIC_CMD_ALM_MN)?))
    }

    fn write_alarm_mn(&self, mn: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_ALM_MN, dec_to_bcd(mn))
    }

    pub fn read_alarm_ss(&self) -> Result<u8> {
        Ok(bcd_to_dec(self.i2c_read_byte(IIC_CMD_ALM_SS)?))
    }

    fn write_alarm_ss(&self, ss: u8) -> Result<()> {
        self.i2c_write_byte(IIC_CMD_ALM_SS, dec_to_bcd(ss))
    }

    fn read_rtc_addr(&self) -> Result<u8> {
        self.i2c_read_byte(IIC_CMD_RTC_ADDR)
    }

    fn set_rtc_addr(&self, addr: u8) -> Result<()> {
        if addr < 0x03 || addr > 0x77 {
            return Err(Error::Other("Invalid RTC I2C address".to_string()));
        }
        let addr = if addr.count_ones() % 2 == 0 {
            addr
        } else {
            addr | (1 << 7)
        };
        self.i2c_write_byte(IIC_CMD_RTC_ADDR, addr)
    }

    pub fn read_app_version(&self) -> Result<String> {
        let mut buf = [0; APP_VER_LEN + 1];
        let mut last = APP_VER_LEN - 1;
        for i in 0..APP_VER_LEN {
            buf[i] = self.i2c_read_byte(IIC_CMD_APPVER + i as u8)?;
            if buf[i] == 0 {
                last = i;
                break;
            }
        }
        log::debug!("ver: {:?}", buf);
        CStr::from_bytes_with_nul(&buf[..(last + 1)])
            .map(|cstr| cstr.to_string_lossy().to_string())
            .map_err(|_| Error::Other("Invalid firmware version".to_string()))
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
    version: String,
    cfg: PiSugarConfig,
}

impl PiSugar3Battery {
    pub fn new(cfg: PiSugarConfig, model: Model) -> Result<Self> {
        let pisugar3 = PiSugar3::new(cfg.i2c_bus, cfg.i2c_addr.unwrap_or(model.default_battery_i2c_addr()))?;
        let poll_at = Instant::now() - std::time::Duration::from_secs(10);
        Ok(Self {
            pisugar3,
            model,
            voltages: VecDeque::with_capacity(30),
            intensities: VecDeque::with_capacity(30),
            levels: VecDeque::with_capacity(30),
            poll_at,
            version: "".to_string(),
            cfg,
        })
    }
}

impl Battery for PiSugar3Battery {
    fn init(&mut self, config: &PiSugarConfig) -> crate::Result<()> {
        log::debug!("Toggle soft poweroff");
        self.pisugar3.toggle_soft_poweroff(config.soft_poweroff == Some(true))?;

        log::debug!("Toggle power restore");
        self.pisugar3.toggle_restore(config.auto_power_on == Some(true))?;

        log::debug!("Toggle anti-mistouch");
        if let Some(anti_mistouch) = config.anti_mistouch {
            self.toggle_anti_mistouch(anti_mistouch)?;
        }

        log::debug!("Toggle bat protect");
        if let Some(protect) = config.bat_protect {
            self.toggle_input_protected(protect)?;
        }

        self.version = self.pisugar3.read_app_version()?;

        Ok(())
    }

    fn model(&self) -> String {
        self.model.to_string()
    }

    fn led_amount(&self) -> crate::Result<u32> {
        Ok(self.model.led_amount())
    }

    fn version(&self) -> Result<String> {
        Ok(self.version.clone())
    }

    fn keep_input(&self) -> Result<bool> {
        let v = self.pisugar3.read_bat_ctr2()?;
        Ok((v & 1 << 7) != 0)
    }

    fn set_keep_input(&self, enable: bool) -> Result<()> {
        let mut v = self.pisugar3.read_bat_ctr2()?;
        if enable {
            v |= 1 << 7;
        } else {
            v &= !(1 << 7);
        }
        self.pisugar3.write_bat_ctr2(v)?;
        Ok(())
    }

    fn voltage(&self) -> crate::Result<f32> {
        let v = self.pisugar3.read_voltage()?;
        Ok((v as f32) / 1000.0)
    }

    fn voltage_avg(&self) -> crate::Result<f32> {
        let mut total = 0.0;
        self.voltages.iter().for_each(|v| total += v.1);
        if !self.voltages.is_empty() {
            Ok(total / self.voltages.len() as f32)
        } else {
            Err(Error::Other("Require initialization".to_string()))
        }
    }

    fn level(&self) -> crate::Result<f32> {
        let curve = self
            .cfg
            .battery_curve
            .as_ref()
            .map(|x| &x[..])
            .unwrap_or(BATTERY_CURVE.as_ref());
        self.voltage_avg().map(|v| IP5312::parse_voltage_level(v, curve))
    }

    fn intensity(&self) -> crate::Result<f32> {
        let c = self.pisugar3.read_output_current()?;
        Ok((c as f32) / 1000.0)
    }

    fn intensity_avg(&self) -> crate::Result<f32> {
        let mut total = 0.0;
        self.intensities.iter().for_each(|i| total += i.1);
        if !self.intensities.is_empty() {
            Ok(total / self.intensities.len() as f32)
        } else {
            Err(Error::Other("Require initialization".to_string()))
        }
    }

    fn is_power_plugged(&self) -> crate::Result<bool> {
        let ctr1 = self.pisugar3.read_ctr1()?;
        Ok((ctr1 & (1 << 7)) != 0)
    }

    fn toggle_power_restore(&self, enable: bool) -> Result<()> {
        self.pisugar3.toggle_restore(enable)
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
        Ok(power_plugged && allow_charging)
    }

    fn is_input_protected(&self) -> Result<bool> {
        self.pisugar3.read_bat_input_protected()
    }

    fn toggle_input_protected(&self, enable: bool) -> Result<()> {
        self.pisugar3.toggle_bat_input_protected(enable)
    }

    fn output_enabled(&self) -> Result<bool> {
        self.pisugar3.read_output_enabled()
    }

    fn toggle_output_enabled(&self, enable: bool) -> Result<()> {
        self.pisugar3.toggle_output_enabled(enable)
    }

    fn poll(&mut self, now: Instant, config: &PiSugarConfig) -> crate::Result<Vec<BatteryEvent>> {
        // slow down, 500ms
        if self.poll_at > now || self.poll_at + std::time::Duration::from_millis(500) > now {
            return Ok(Vec::default());
        }
        self.poll_at = now;

        let voltage = self.voltage()?;
        self.voltages.pop_front();
        while self.voltages.len() < self.voltages.capacity() {
            self.voltages.push_back((now, voltage));
        }

        let level = self.level()?;
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

        // soft poweroff
        let mut soft_poweroff = false;
        if config.soft_poweroff == Some(true) {
            match self.pisugar3.read_soft_poweroff_flag() {
                Ok(f) => {
                    soft_poweroff = f;
                    if f {
                        let _ = self.pisugar3.clear_soft_poweroff_flag();
                    }
                }
                Err(e) => log::warn!("Read soft poweroff flag error: {}", e),
            }
        }

        let mut events = Vec::new();
        if let Some(tap) = tap {
            events.push(BatteryEvent::TapEvent(tap));
        }
        if soft_poweroff {
            events.push(BatteryEvent::SoftPowerOff);
        }

        Ok(events)
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

    fn toggle_soft_poweroff(&self, enable: bool) -> Result<()> {
        self.pisugar3.toggle_soft_poweroff(enable)
    }

    fn toggle_anti_mistouch(&self, enable: bool) -> Result<()> {
        let mut ctr1 = self.pisugar3.read_ctr1()?;
        ctr1 &= 0b1111_0111;
        if enable {
            ctr1 |= 0b0000_1000;
        }
        self.pisugar3.write_ctr1(ctr1)?;
        Ok(())
    }

    fn temperature(&self) -> Result<f32> {
        Ok(self.pisugar3.read_temp()? as f32)
    }
}

pub struct PiSugar3RTC {
    pisugar3: PiSugar3,
    cfg: PiSugarConfig,
}

impl PiSugar3RTC {
    pub fn new(cfg: PiSugarConfig, model: Model) -> Result<Self> {
        let pisugar3 = PiSugar3::new(cfg.i2c_bus, model.default_rtc_i2c_addr())?;
        Ok(Self { pisugar3, cfg })
    }
}

impl RTC for PiSugar3RTC {
    fn init(&mut self, config: &PiSugarConfig) -> Result<()> {
        self.pisugar3.toggle_restore(config.auto_power_on == Some(true))?;
        self.pisugar3.toggle_alarm_enable(false)?;
        if let Some(wakeup_time) = config.auto_wake_time {
            if config.auto_wake_repeat & 0x7f != 0 {
                self.set_alarm(wakeup_time.into(), config.auto_wake_repeat)?;
                self.pisugar3.toggle_alarm_enable(true)?;
            }
        }
        if let Some(adj_comm) = config.adj_comm {
            self.pisugar3.write_rtc_adj_comm(adj_comm)?;
        }
        if let Some(adj_diff) = config.adj_diff {
            self.pisugar3.write_rtc_adj_diff(adj_diff)?;
        }
        Ok(())
    }

    fn read_addr(&self) -> Result<u8> {
        self.pisugar3.read_rtc_addr()
    }

    fn set_addr(&self, addr: u8) -> Result<()> {
        if addr < 0x03 || addr > 0x77 {
            return Err(Error::Other("Invalid RTC I2C address".to_string()));
        }
        self.pisugar3.set_rtc_addr(addr)
    }

    fn read_time(&self) -> Result<RTCRawTime> {
        Ok(RTCRawTime::from_dec([
            self.pisugar3.read_rtc_ss()?,
            self.pisugar3.read_rtc_mn()?,
            self.pisugar3.read_rtc_hh()?,
            self.pisugar3.read_rtc_weekday()?,
            self.pisugar3.read_rtc_dd()?,
            self.pisugar3.read_rtc_mm()?,
            self.pisugar3.read_rtc_yy()?,
        ]))
    }

    fn write_time(&self, raw: RTCRawTime) -> Result<()> {
        self.pisugar3.write_rtc_ss(raw.second())?;
        self.pisugar3.write_rtc_mn(raw.minute())?;
        self.pisugar3.write_rtc_hh(raw.hour())?;
        self.pisugar3.write_rtc_weekday(raw.weekday())?;
        self.pisugar3.write_rtc_dd(raw.day())?;
        self.pisugar3.write_rtc_mm(raw.month())?;
        self.pisugar3.write_rtc_yy(((raw.year() - 2000) & 0xff) as u8)?;
        Ok(())
    }

    fn write_adjust_ppm(&self, ppm: f64) -> Result<()> {
        let ppm_abs = ppm.abs();
        let adj = ppm_abs * 32000000.0 / 30.517;
        let comm = adj / 32.0;
        let comm = if comm > 15.0 { 15 } else { comm as u8 };
        let diff = adj - comm as f64 * 32.0;
        let diff = if diff > 31.0 { 31 } else { diff as u8 };

        if ppm > 0.0 {
            self.pisugar3.write_rtc_adj_comm(comm | 1 << 7)?;
        } else {
            self.pisugar3.write_rtc_adj_comm(comm)?;
        }
        self.pisugar3.write_rtc_adj_diff(diff)?;

        Ok(())
    }

    fn read_alarm_time(&self) -> Result<RTCRawTime> {
        let mut raw = RTCRawTime::from_dec([
            self.pisugar3.read_alarm_ss()?,
            self.pisugar3.read_alarm_mn()?,
            self.pisugar3.read_alarm_hh()?,
            0,
            1,
            1,
            0,
        ]);
        raw.0[3] = self.pisugar3.read_alarm_weekday_repeat()?;
        Ok(raw)
    }

    fn set_alarm(&self, time: RTCRawTime, weekday_repeat: u8) -> Result<()> {
        self.pisugar3.write_alarm_hh(time.hour())?;
        self.pisugar3.write_alarm_mn(time.minute())?;
        self.pisugar3.write_alarm_ss(time.second())?;
        self.pisugar3.write_alarm_weekday_repeat(weekday_repeat)?;
        self.pisugar3.toggle_alarm_enable(true)?;
        Ok(())
    }

    fn is_alarm_enable(&self) -> Result<bool> {
        self.pisugar3.get_alarm_enable()
    }

    fn toggle_alarm_enable(&self, enable: bool) -> Result<()> {
        self.pisugar3.toggle_alarm_enable(enable)
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
