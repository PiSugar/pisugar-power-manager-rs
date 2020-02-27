#[macro_use]
extern crate num_derive;

use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::thread;
use std::thread::Thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use num_traits::{FromPrimitive, ToPrimitive};
use rppal::i2c::Error as I2cError;
use rppal::i2c::I2c;

const TIME_HOST: &str = "cdn.pisugar.com";

/// RTC address, SD3078
const I2C_ADDR_RTC: u16 = 0x32;
const I2C_RTC_CTR1: u8 = 0x0f;
const I2C_RTC_CTR2: u8 = 0x10;
const I2C_RTC_CTR3: u8 = 0x11;

/// Battery address, IP5209 or IP5312
const I2C_ADDR_BAT: u16 = 0x75;
const I2C_BAT_INTENSITY_LOW: u8 = 0xa4;
const I2C_BAT_INTENSITY_HIGH: u8 = 0xa5;
const I2C_BAT_VOLTAGE_LOW: u8 = 0xa2;
const I2C_BAT_VOLTAGE_HIGH: u8 = 0xa3;
const I2C_BAT_P_INTENSITY_LOW: u8 = 0x66;
const I2C_BAT_P_INTENSITY_HIGH: u8 = 0x67;
const I2C_BAT_P_VOLTAGE_LOW: u8 = 0x64;
const I2C_BAT_P_VOLTAGE_HIGH: u8 = 0x65;

const I2C_READ_INTERVAL: Duration = Duration::from_secs(1);

pub const MODEL_V2: &str = "PiSugar 2";
pub const MODEL_V2_PRO: &str = "PiSugar 2 Pro";

/// PiSugar error
#[derive(Debug)]
pub enum Error {
    I2c(I2cError),
}

/// Convert I2cError to PiSugar error
impl From<I2cError> for Error {
    fn from(e: I2cError) -> Self {
        Error::I2c(e)
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
    log::debug!("voltage low: 0x{:x}, high: 0x{:x}", low, high);

    // check negative values
    let voltage = if high & 0x20 == 0x20 {
        let v = (!(((high & 0x1f) << 8) + low) as i16) + 1;
        2600.0 - (v as f64) * 0.26855
    } else {
        let v = ((high & 0x1f) << 8) + low;
        2600.0 + v as f64 * 0.26855
    };

    Ok(voltage / 1000.0)
}

/// Read battery current intensity
pub fn bat_read_intensity() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;

    let low = i2c.smbus_read_byte(I2C_BAT_INTENSITY_LOW)? as u16;
    let high = i2c.smbus_read_byte(I2C_BAT_INTENSITY_HIGH)? as u16;
    log::debug!("intensity low: 0x{:x}, high: 0x{:x}", low, high);

    let intensity = if high & 0x20 == 0x20 {
        let v = (!(((high & 0x1f) << 8) + low) as i16) + 1;
        -(v as f64) * 0.745985
    } else {
        let v = ((high & 0x1f) << 8) + low;
        v as f64 * 0.745985
    };

    Ok(intensity / 1000.0)
}

/// Read battery pro intensity
pub fn bat_p_read_intensity() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;
    let low = i2c.smbus_read_byte(I2C_BAT_P_INTENSITY_LOW)?;
    let high = i2c.smbus_read_byte(I2C_BAT_P_INTENSITY_HIGH)?;
    let intensity = if high & 0x20 != 0 {
        let low = (!low) as u16;
        let high = (!high & 0x1f) as u16;
        -((high << 8 + low + 1) as f64) * 1.27883
    } else {
        let low = low as u16;
        let high = (high & 0x1f) as u16;
        (high << 8 + low + 1) as f64 * 1.27883
    };
    Ok(intensity)
}

/// Read battery pro voltage
pub fn bat_p_read_voltage() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;
    let low = i2c.smbus_read_byte(I2C_BAT_P_VOLTAGE_LOW)?;
    let high = i2c.smbus_read_byte(I2C_BAT_P_VOLTAGE_HIGH)?;
    let voltage = if high & 0x20 != 0 {
        let low = (!low) as u16;
        let high = (!high & 0x1f) as u16;
        -((high << 8 + low + 1) as f64) * 0.26855 + 2600.0
    } else {
        let low = low as u16;
        let high = (high & 0x1f) as u16;
        (high << 8 + low + 1) as f64 * 0.26855 + 2600.0
    };
    Ok(voltage)
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

pub fn bat_p_set_shutdown_threshold() -> Result<()> {
    unimplemented!()
}

pub fn bat_p_force_shutdown() -> Result<()> {
    unimplemented!()
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

pub fn bat_read_gpio() -> Result<u8> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;
    let v = i2c.smbus_read_byte(0x55)?;
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

fn bcd_to_dec_list(bcd_list: &Vec<u8>) -> Vec<u8> {
    let mut list = Vec::with_capacity(bcd_list.len());
    for bcd in bcd_list {
        list.push(bcd_to_dec(bcd.clone()));
    }
    list
}

fn dec_to_bcd_list(dec_list: &Vec<u8>) -> Vec<u8> {
    let mut list = Vec::with_capacity(dec_list.len());
    for dec in dec_list {
        list.push(dec_to_bcd(dec.clone()));
    }
    list
}

fn datetime_to_bcd(datetime: DateTime<Local>) -> [u8; 7] {
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

fn bcd_to_datetime(bcd_time: &[u8; 7]) -> DateTime<Local> {
    let sec = bcd_to_dec(bcd_time[0]) as u32;
    let min = bcd_to_dec(bcd_time[1]) as u32;
    let hour = bcd_to_dec(bcd_time[2]) as u32;
    let day_of_month = bcd_to_dec(bcd_time[4]) as u32;
    let month = bcd_to_dec(bcd_time[5]) as u32;
    let year = 2000 + bcd_to_dec(bcd_time[6]) as i32;

    let datetime = Local.ymd(year, month, day_of_month).and_hms(hour, min, sec);
    datetime
}

pub struct RtcDateTime(pub [u8; 7]);

impl RtcDateTime {
    pub fn from_raw_bcd(raw_bcd: &[u8; 7]) -> Self {
        Self(raw_bcd.clone())
    }

    pub fn from_datetime(datetime: DateTime<Local>) -> Self {
        unimplemented!()
    }

    pub fn new() -> Self {
        Self([0; 7])
    }

    pub fn seconds(&self) -> u8 {
        bcd_to_dec(self.0[0])
    }

    pub fn set_seconds(&mut self, seconds: u8) {
        self.0[0] = dec_to_bcd(seconds)
    }

    pub fn minus(&self) -> u8 {
        bcd_to_dec(self.0[1])
    }

    pub fn set_minus(&mut self, minus: u8) {
        self.0[1] = dec_to_bcd(minus)
    }

    pub fn hour(&self) -> u8 {
        // 24hr
        if self.0[2] & 0b1000_0000 != 0 {
            bcd_to_dec(self.0[2] & 0b0111_1111) // 24hr
        } else if self.0[2] & 0b0010_0000 != 0 {
            12 + bcd_to_dec(self.0[2] & 0b0001_1111) // 12hr, pm
        } else {
            bcd_to_dec(self.0[2]) // 12hr, am
        }
    }

    pub fn weekday(&self) -> u8 {
        bcd_to_dec(self.0[3])
    }

    pub fn day(&self) -> u8 {
        bcd_to_dec(self.0[4])
    }

    pub fn month(&self) -> u8 {
        bcd_to_dec(self.0[5])
    }

    pub fn year(&self) -> u8 {
        bcd_to_dec(self.0[6])
    }

    pub fn to_local(&self) -> DateTime<Local> {
        bcd_to_datetime(&self.0)
    }
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

/// PiSugar configuation
pub struct PiSugarConfig {
    /// Auto wakeup type
    pub auto_wake_type: String,
    pub auto_wake_time: String,
    pub auto_wake_repeat: String,
    pub single_tap_enable: bool,
    pub single_tap_shell: String,
    pub double_tap_enable: String,
    pub double_tap_shell: String,
    pub long_tap_enable: String,
    pub long_tap_shell: String,
    pub auto_shutdown_percent: String,
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
}

impl PiSugarStatus {
    pub fn new() -> Self {
        let mut level_records = VecDeque::with_capacity(10);

        let mut model = String::from(MODEL_V2_PRO);
        let voltage = match bat_read_voltage() {
            Ok(voltage) => {
                model = String::from(MODEL_V2);
                voltage
            }
            _ => 0.0,
        };
        let level = convert_battery_voltage_to_level(voltage);

        let intensity = match bat_read_intensity() {
            Ok(intensity) => intensity,
            _ => 0.0,
        };

        for i in 0..level_records.capacity() {
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
        if self.updated_at + 3 * I2C_READ_INTERVAL >= now {
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
}

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
