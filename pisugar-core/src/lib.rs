#[macro_use]
extern crate num_derive;

use std::collections::VecDeque;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::ops::Add;
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::sync::{Arc, RwLock};
use std::thread;
use std::thread::Thread;
use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, Datelike, FixedOffset, Local, TimeZone, Timelike};
use num_traits::{FromPrimitive, ToPrimitive};
use rppal::i2c::Error as I2cError;
use rppal::i2c::I2c;
use serde::export::Result::Err;
use serde::{Deserialize, Serialize};

const TIME_HOST: &str = "cdn.pisugar.com";

// RTC address, SD3078
const I2C_ADDR_RTC: u16 = 0x32;
const I2C_RTC_CTR1: u8 = 0x0f;
const I2C_RTC_CTR2: u8 = 0x10;
const I2C_RTC_CTR3: u8 = 0x11;

// Battery address, IP5209 or IP5312
const I2C_ADDR_BAT: u16 = 0x75;
const I2C_BAT_INTENSITY_LOW: u8 = 0xa4;
const I2C_BAT_INTENSITY_HIGH: u8 = 0xa5;
const I2C_BAT_VOLTAGE_LOW: u8 = 0xa2;
const I2C_BAT_VOLTAGE_HIGH: u8 = 0xa3;

pub const MODEL_V2: &str = "PiSugar 2";
pub const MODEL_V2_PRO: &str = "PiSugar 2 Pro";

/// PiSugar error
#[derive(Debug)]
pub enum Error {
    I2c(I2cError),
    Other(String),
}

/// Wrap I2cError
impl From<I2cError> for Error {
    fn from(e: I2cError) -> Self {
        Error::I2c(e)
    }
}

impl From<String> for Error {
    fn from(e: String) -> Self {
        Error::Other(e)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::I2c(e) => write!(f, "{}", e),
            Error::Other(e) => write!(f, "{}", e),
        }
    }
}

/// PiSugar result
pub type Result<T> = std::result::Result<T, Error>;

/// Battery voltage threshold, (low, high, percentage at low, percentage at high)
type BatteryThreshold = (f64, f64, f64, f64);

/// Battery threshold curve
const BATTERY_CURVE: [BatteryThreshold; 11] = [
    (4.16, 5.5, 100.0, 100.0),
    (4.05, 4.16, 87.5, 100.0),
    (4.00, 4.05, 75.0, 87.5),
    (3.92, 4.00, 62.5, 75.0),
    (3.86, 3.92, 50.0, 62.5),
    (3.79, 3.86, 37.5, 50.0),
    (3.66, 3.79, 25.0, 37.5),
    (3.52, 3.66, 12.5, 25.0),
    (3.49, 3.52, 6.2, 12.5),
    (3.1, 3.49, 0.0, 6.2),
    (0.0, 3.1, 0.0, 0.0),
];

/// Battery voltage to percentage level
fn convert_battery_voltage_to_level(voltage: f64) -> f64 {
    if voltage > 5.5 {
        return 100.0;
    }
    for threshold in &BATTERY_CURVE {
        if voltage >= threshold.0 {
            let mut percentage = (voltage - threshold.0) / (threshold.1 - threshold.0);
            let level = threshold.2 + percentage * (threshold.3 - threshold.2);
            return level;
        }
    }
    0.0
}

/// Read battery voltage
pub fn bat_read_voltage() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;

    let low = i2c.smbus_read_byte(I2C_BAT_VOLTAGE_LOW)? as u16;
    let high = i2c.smbus_read_byte(I2C_BAT_VOLTAGE_HIGH)? as u16;
    log::debug!("voltage low 0x{:x}, high 0x{:x}", low, high);

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

/// Read battery current intensity
pub fn bat_read_intensity() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;

    let low = i2c.smbus_read_byte(0xa4)? as u16;
    let high = i2c.smbus_read_byte(0xa5)? as u16;
    log::debug!("intensity low 0x{:x}, high 0x{:x}", low, high);

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

/// Read battery pro intensity
pub fn bat_p_read_intensity() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;

    let low = i2c.smbus_read_byte(0xd2)? as u16;
    let high = i2c.smbus_read_byte(0xd3)? as u16;
    log::debug!("intensity low 0x{:x}, high 0x{:x}", low, high);

    let intensity = if high & 0x20 != 0 {
        let i = (((high | 0b1100_0000) << 8) + low) as i16;
        (i as f64) * 2.68554
    } else {
        let i = ((high & 0x1f) << 8) + low;
        (i as f64) * 2.68554
    };
    Ok(intensity / 1000.0)
}

/// Read battery pro voltage, use this to detect the model
pub fn bat_p_read_voltage() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;
    let low = i2c.smbus_read_byte(0xd0)? as u16;
    let high = i2c.smbus_read_byte(0xd1)? as u16;
    log::debug!("voltage low 0x{:x}, high 0x{:x}", low, high);

    if low == 0 && high == 0 {
        return Err(Error::I2c(I2cError::FeatureNotSupported));
    }

    let v = ((high & 0b0011_1111) + low);
    let v = (v as f64) * 0.26855 + 2600.0;
    Ok(v / 1000.0)
}

/// Set shutdown threshold
pub fn bat_set_shutdown_threshold() -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;

    // threshold intensity
    let mut v = i2c.smbus_read_byte(0x0c)?;
    v &= 0b0000_0111;
    v |= (12 << 3);
    i2c.smbus_write_byte(0x0c, v)?;

    // time
    let mut v = i2c.smbus_read_byte(0x04)?;
    v |= 0b0000_0000;
    v &= 0b00111111;
    i2c.smbus_write_byte(0x04, v)?;

    // enable
    let mut v = i2c.smbus_read_byte(0x02)?;
    v |= 0b0000_0011;
    i2c.smbus_write_byte(0x02, v)?;

    Ok(())
}

/// Set shutdown threshold of P
pub fn bat_p_set_shutdown_threshold() -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;

    // threshold intensity
    let mut t = i2c.smbus_read_byte(0x84)?;
    t &= 0b0000_0111;
    t |= (12 << 3);
    t = 0xFF;
    i2c.smbus_write_byte(0x84, t)?;

    // time
    t = i2c.smbus_read_byte(0x07)?;
    t |= 0b0100_0000;
    t &= 0b0111_1111;
    i2c.smbus_write_byte(0x07, t)?;

    Ok(())
}

pub fn bat_p_force_shutdown() -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;

    // enable force shutdown
    let mut t = i2c.smbus_read_byte(0x5B)?;
    t |= 0b0001_0010;
    i2c.smbus_write_byte(0x5B, t)?;

    // force shutdown
    t = i2c.smbus_read_byte(0x5B)?;
    t &= 0b1110_1111;
    i2c.smbus_write_byte(0x5B, t)?;

    Ok(())
}

pub fn bat_set_gpio() -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;

    // vset
    let mut v = i2c.smbus_read_byte(0x26)?;
    v |= 0b0000_0000;
    v &= 0b1011_1111;
    i2c.smbus_write_byte(0x26, v)?;

    // vset -> gpio
    let mut v = i2c.smbus_read_byte(0x52)?;
    v |= 0b0000_0100;
    v &= 0b1111_0111;
    i2c.smbus_write_byte(0x52, v)?;

    // gpio input
    let mut v = i2c.smbus_read_byte(0x53)?;
    v |= 0b0001_0000;
    v &= 0b1111_1111;
    i2c.smbus_write_byte(0x53, v)?;

    Ok(())
}

pub fn bat_p_set_gpio() -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;

    // mfp_ctl0, set l4_sel
    let mut v = i2c.smbus_read_byte(0x52)?;
    v |= 0b0000_0010;
    i2c.smbus_write_byte(0x52, v)?;

    // gpio1 input
    let mut v = i2c.smbus_read_byte(0x54)?;
    v |= 0b0000_0010;
    i2c.smbus_write_byte(0x54, v)?;

    Ok(())
}

pub fn bat_read_gpio_tap() -> Result<u8> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;
    let v = i2c.smbus_read_byte(0x55)?;
    Ok(v)
}

pub fn bat_p_read_gpio_tap() -> Result<u8> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;
    let mut v = i2c.smbus_read_byte(0x58)?;
    v &= 0b0000_0010;
    Ok(v)
}

pub fn rtc_disable_write_protect() -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_RTC)?;

    let mut data = i2c.smbus_read_byte(I2C_RTC_CTR2)?;
    data |= 0b1000_0000;
    i2c.smbus_write_byte(I2C_RTC_CTR2, data);

    data = i2c.smbus_read_byte(I2C_RTC_CTR1)?;
    data |= 0b1000_0100;
    i2c.smbus_write_byte(I2C_RTC_CTR1, data)?;

    Ok(())
}

pub fn rtc_enable_write_protect() -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_RTC)?;

    let mut data = i2c.smbus_read_byte(I2C_RTC_CTR1)?;
    data &= 0b0111_1011;
    i2c.smbus_write_byte(I2C_RTC_CTR1, data);

    data = i2c.smbus_read_byte(I2C_RTC_CTR2)?;
    data &= 0b0111_1111;
    i2c.smbus_write_byte(I2C_RTC_CTR2, data)?;

    Ok(())
}

pub fn rtc_read_alarm_flag() -> Result<bool> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_RTC)?;

    let data = i2c.smbus_read_byte(I2C_RTC_CTR1)?;
    if data & 0b0010_0000 != 0 || data & 0b0001_0000 != 0 {
        return Ok(true);
    }

    Ok(false)
}

pub fn rtc_clean_alarm_flag() -> Result<()> {
    match rtc_read_alarm_flag() {
        Ok(true) => {
            rtc_disable_write_protect()?;
            let mut i2c = I2c::new()?;
            i2c.set_slave_address(I2C_ADDR_RTC)?;

            let mut data = i2c.smbus_read_byte(I2C_RTC_CTR1)?;
            data &= 0b1100_1111;
            i2c.smbus_write_byte(I2C_RTC_CTR1, data)?;

            rtc_enable_write_protect()?;
        }
        _ => {}
    }
    Ok(())
}

fn bcd_to_dec(bcd: u8) -> u8 {
    (bcd & 0x0F) + (((bcd & 0xF0) >> 4) * 10)
}

fn dec_to_bcd(dec: u8) -> u8 {
    dec % 10 + ((dec / 10) << 4)
}

pub fn datetime_to_bcd(datetime: DateTime<Local>) -> [u8; 7] {
    let mut bcd_time = [0_u8; 7];
    bcd_time[0] = (dec_to_bcd(datetime.second() as u8));
    bcd_time[1] = (dec_to_bcd(datetime.minute() as u8));
    bcd_time[2] = (dec_to_bcd(datetime.hour() as u8));
    bcd_time[3] = (dec_to_bcd(datetime.weekday().num_days_from_sunday() as u8));
    bcd_time[4] = (dec_to_bcd(datetime.day() as u8));
    bcd_time[5] = (dec_to_bcd(datetime.month() as u8));
    bcd_time[6] = (dec_to_bcd((datetime.year() % 100) as u8));
    bcd_time
}

pub fn bcd_to_datetime(bcd_time: &[u8; 7]) -> DateTime<Local> {
    let sec = bcd_to_dec(bcd_time[0]) as u32;
    let min = bcd_to_dec(bcd_time[1]) as u32;
    let hour = bcd_to_dec(bcd_time[2]) as u32;
    let day_of_month = bcd_to_dec(bcd_time[4]) as u32;
    let month = bcd_to_dec(bcd_time[5]) as u32;
    let year = 2000 + bcd_to_dec(bcd_time[6]) as i32;

    let datetime = Local.ymd(year, month, day_of_month).and_hms(hour, min, sec);
    datetime
}

pub fn sys_write_time(dt: DateTime<Local>) {
    let cmd = format!(
        "/bin/date -s {}-{}-{} {}:{}:{}",
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    );
    execute_shell(cmd.as_str());
    let cmd = "/sbin/hwclock -w";
    execute_shell(cmd);
}

pub fn rtc_write_time(bcd_time: &[u8; 7]) -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_RTC)?;

    // 24h
    let mut bcd_time = bcd_time.clone();
    bcd_time[2] |= 0b1000_0000;

    rtc_disable_write_protect()?;
    i2c.block_write(0, bcd_time.as_ref());
    rtc_enable_write_protect()?;

    Ok(())
}

pub fn rtc_read_time() -> Result<[u8; 7]> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_RTC)?;

    let mut bcd_time = [0_u8; 7];
    i2c.block_read(0, &mut bcd_time)?;

    // 12hr or 24hr
    if bcd_time[2] & 0b1000_0000 != 0 {
        bcd_time[2] &= 0b0111_1111; // 24hr
    } else if bcd_time[2] & 0b0010_0000 != 0 {
        bcd_time[2] += 12; // 12hr and pm
    }

    Ok(bcd_time)
}

pub fn rtc_set_alarm(bcd_time: &[u8; 7], weekday_repeat: u8) -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_RTC)?;

    let mut bcd_time = bcd_time.clone();
    bcd_time[3] = weekday_repeat;

    rtc_disable_write_protect()?;
    i2c.block_write(0x07, bcd_time.as_ref())?;

    let mut ctr2 = i2c.smbus_read_byte(I2C_RTC_CTR2)?;
    ctr2 |= 0b0101_0010;
    ctr2 &= 0b1101_1111;
    i2c.smbus_write_byte(I2C_RTC_CTR2, ctr2)?;

    // alarm allows hour/minus/second
    i2c.smbus_write_byte(0x0e, 0b0000_0111);

    rtc_enable_write_protect()?;

    Ok(())
}

pub fn rtc_disable_alarm() -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_RTC)?;

    rtc_disable_write_protect()?;

    let mut ctr2 = i2c.smbus_read_byte(I2C_RTC_CTR2)?;
    ctr2 |= 0b0101_0010;
    ctr2 &= 0b1101_1111;
    i2c.smbus_write_byte(I2C_RTC_CTR2, ctr2)?;

    i2c.smbus_write_byte(0x0e, 0b0000_0000);

    rtc_enable_write_protect()?;

    Ok(())
}

pub fn rtc_set_test_wake() -> Result<()> {
    log::info!("wakeup after 1min30sec");
    let now = Local::now();
    let duration = chrono::Duration::seconds(90);
    let bcd_time = datetime_to_bcd(now);
    rtc_write_time(&bcd_time).and_then(|_| {
        let then = now + duration;
        let bcd_time_then = datetime_to_bcd(then);
        rtc_set_alarm(&bcd_time, 0b0111_1111)
    })
}

/// PiSugar configuration
#[derive(Default, Serialize, Deserialize)]
pub struct PiSugarConfig {
    /// Auto wakeup type
    pub auto_wake_type: i32,
    pub auto_wake_time: [u8; 7],
    pub auto_wake_repeat: u8,
    pub single_tap_enable: bool,
    pub single_tap_shell: String,
    pub double_tap_enable: bool,
    pub double_tap_shell: String,
    pub long_tap_enable: bool,
    pub long_tap_shell: String,
    pub auto_shutdown_level: f64,
}

impl PiSugarConfig {
    pub fn load(&mut self, path: &Path) -> io::Result<()> {
        let mut f = File::open(path)?;
        let mut buff = String::new();
        let _ = f.read_to_string(&mut buff)?;
        let config = serde_json::from_str(&buff)?;
        *self = config;
        Ok(())
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let mut f = File::open(path)?;
        let s = serde_json::to_string(self)?;
        f.write_all(s.as_bytes())
    }
}

/// PiSugar status
pub struct PiSugarStatus {
    model: String,
    voltage: f64,
    intensity: f64,
    level: f64,
    level_records: VecDeque<f64>,
    charging: bool,
    updated_at: Instant,
    rtc_time: DateTime<Local>,
    rtc_time_list: [u8; 6],
    gpio_tap_history: String,
}

impl PiSugarStatus {
    pub fn new() -> Self {
        let mut level_records = VecDeque::with_capacity(10);

        let mut model = String::from(MODEL_V2);
        let mut voltage = 0.0;
        let mut intensity = 0.0;

        if let Ok(v) = bat_p_read_voltage() {
            log::info!("Read pro voltage success");
            model = String::from(MODEL_V2_PRO);
            voltage = v;
            intensity = bat_read_intensity().unwrap_or(0.0);

            if bat_p_set_gpio().is_ok() {
                log::info!("Set gpio tap success");
            } else {
                log::error!("Set gpio tap failed")
            }
        } else if let Ok(v) = bat_read_voltage() {
            log::info!("Read zero voltage success");
            model = String::from(MODEL_V2);
            voltage = v;
            intensity = bat_p_read_intensity().unwrap_or(0.0);

            if bat_set_gpio().is_ok() {
                log::info!("Set gpio tap success");
            } else {
                log::error!("Set gpio tap failed");
            }
        } else {
            log::warn!("PiSugar not found");
        }

        let level = convert_battery_voltage_to_level(voltage);
        for _ in 0..level_records.capacity() {
            level_records.push_back(level);
        }

        Self {
            model,
            voltage,
            intensity,
            level,
            level_records,
            charging: false,
            updated_at: Instant::now(),
            rtc_time: Local::now(),
            rtc_time_list: [0; 6],
            gpio_tap_history: String::with_capacity(10),
        }
    }

    /// PiSugar model
    pub fn mode(&self) -> &str {
        self.model.as_str()
    }

    /// Battery level
    pub fn level(&self) -> f64 {
        self.level
    }

    /// Battery voltage
    pub fn voltage(&self) -> f64 {
        self.voltage
    }

    /// Update battery voltage
    pub fn update_voltage(&mut self, voltage: f64, now: Instant) {
        self.updated_at = now;
        self.voltage = voltage;
        self.level = convert_battery_voltage_to_level(voltage);
        self.level_records.pop_front();
        self.level_records.push_back(self.level);
    }

    /// Battery intensity
    pub fn intensity(&self) -> f64 {
        self.intensity
    }

    /// Update battery intensity
    pub fn update_intensity(&mut self, intensity: f64, now: Instant) {
        self.updated_at = now;
        self.intensity = intensity
    }

    /// PiSugar battery alive
    pub fn is_alive(&self, now: Instant) -> bool {
        if self.updated_at + Duration::from_secs(3) >= now {
            return true;
        }
        false
    }

    /// PiSugar is charging, with voltage linear regression
    pub fn is_charging(&self, now: Instant) -> bool {
        if self.is_alive(now) {
            log::debug!("levels: {:?}", self.level_records);
            let capacity = self.level_records.capacity() as f64;
            let mut x_sum = (0.0 + capacity - 1.0) * capacity / 2.0;
            let x_bar = x_sum / capacity;
            let mut y_sum: f64 = self.level_records.iter().sum();
            let y_bar = y_sum / capacity;
            // k = Sum(yi * (xi - x_bar)) / Sum(xi - x_bar)^2
            let mut iter = self.level_records.iter();
            let mut a = 0.0;
            let mut b = 0.0;
            for i in 0..self.level_records.capacity() {
                let xi = i as f64;
                let yi = iter.next().unwrap().clone();
                a += yi * (xi - x_bar);
                b += (xi - x_bar) * (xi - x_bar);
            }
            let k = a / b;
            log::debug!("charging k: {}", k);
            return k >= 0.01;
        }
        false
    }

    pub fn rtc_time(&self) -> DateTime<Local> {
        self.rtc_time
    }

    pub fn set_rtc_time(&mut self, rtc_time: DateTime<Local>) {
        self.rtc_time = rtc_time
    }

    pub fn poll(&mut self, config: &PiSugarConfig, now: Instant) -> Result<Option<TapType>> {
        if self.gpio_tap_history.len() == self.gpio_tap_history.capacity() {
            self.gpio_tap_history.remove(0);
        }

        // battery
        if self.mode() == MODEL_V2 {
            if let Ok(v) = bat_read_voltage() {
                self.update_voltage(v, now);
            }
            if let Ok(i) = bat_read_intensity() {
                self.update_intensity(i, now);
            }
            if let Ok(t) = bat_read_gpio_tap() {
                log::debug!("gpio button state: {}", t);
                if t != 0 {
                    self.gpio_tap_history.push('1');
                } else {
                    self.gpio_tap_history.push('0');
                }
            }
        } else {
            if let Ok(v) = bat_p_read_voltage() {
                self.update_voltage(v, now)
            }
            if let Ok(i) = bat_p_read_intensity() {
                self.update_intensity(i, now)
            }
            if let Ok(t) = bat_p_read_gpio_tap() {
                log::debug!("gpio button state: {}", t);
                if t != 0 {
                    self.gpio_tap_history.push('1');
                } else {
                    self.gpio_tap_history.push('0');
                }
            }
        }

        // auto shutdown
        if self.level() < config.auto_shutdown_level {
            loop {
                log::error!("low battery, will power off...");
                if let Ok(mut proc) = Command::new("poweroff").spawn() {
                    proc.wait();
                }
                thread::sleep(std::time::Duration::from_millis(3000));
            }
        }

        // rtc
        if let Ok(rtc_time) = rtc_read_time() {
            let rtc_time = bcd_to_datetime(&rtc_time);
            self.set_rtc_time(rtc_time)
        }

        // gpio tap detect
        if let Some(tap_type) = gpio_detect_tap(&mut self.gpio_tap_history) {
            log::debug!("tap detected: {}", tap_type);
            return Ok(Some(tap_type));
        }

        Ok(None)
    }
}

/// Button tap type
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum TapType {
    Single,
    Double,
    Long,
}

impl Display for TapType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TapType::Single => "single",
            TapType::Double => "double",
            TapType::Long => "long",
        };
        write!(f, "{}", s)
    }
}

/// Detect button tap
pub fn gpio_detect_tap(gpio_history: &mut String) -> Option<TapType> {
    let long_pattern = "111111110";
    let double_pattern = vec!["1010", "10010", "10110", "100110", "101110", "1001110"];
    let single_pattern = "1000";

    if gpio_history.contains(long_pattern) {
        gpio_history.clear();
        return Some(TapType::Long);
    }

    for pattern in double_pattern {
        if gpio_history.contains(pattern) {
            gpio_history.clear();
            return Some(TapType::Double);
        }
    }

    if gpio_history.contains(single_pattern) {
        gpio_history.clear();
        return Some(TapType::Single);
    }

    None
}

/// Execute shell with sh
pub fn execute_shell(shell: &str) -> io::Result<ExitStatus> {
    let args = ["-c", shell];
    let mut child = Command::new("/bin/sh").args(&args).spawn()?;
    child.wait()
}

/// Core
pub struct PiSugarCore {
    pub config_path: Option<String>,
    pub config: PiSugarConfig,
    pub status: PiSugarStatus,
}

impl PiSugarCore {
    pub fn new(config: PiSugarConfig) -> Self {
        Self {
            config_path: None,
            config,
            status: PiSugarStatus::new(),
        }
    }

    pub fn load_config(path: &Path) -> Result<Self> {
        if path.exists() && path.is_file() {
            let mut config = PiSugarConfig::default();
            if config.load(path).is_ok() {
                return Ok(Self {
                    config_path: Some(path.to_string_lossy().to_string()),
                    config,
                    status: PiSugarStatus::new(),
                });
            }
        }

        Err(Error::Other("Failed to load config file".to_string()))
    }

    pub fn save_config(&self) -> Result<()> {
        if self.config_path.is_some() {
            let path = Path::new(self.config_path.as_ref().unwrap());
            if self.config.save_to(path).is_ok() {
                return Ok(());
            }
        }
        Err(Error::Other("Failed to save config file".to_string()))
    }
}
