use std::collections::VecDeque;
use std::convert::From;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use rppal::i2c::Error as I2cError;
use rppal::i2c::I2c;
use serde::export::Result::Err;
use serde::{Deserialize, Serialize};

/// Time host
pub const TIME_HOST: &str = "cdn.pisugar.com";

/// I2c poll interval
pub const I2C_READ_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// RTC address, SD3078
const I2C_ADDR_RTC: u16 = 0x32;

/// Battery address, IP5209/IP5312
const I2C_ADDR_BAT: u16 = 0x75;

pub const MODEL_V2: &str = "PiSugar 2";
pub const MODEL_V2_PRO: &str = "PiSugar 2 Pro";

const PI_ZERO_IDLE_INTENSITY: f64 = 0.12;
const PI_PRO_IDLE_INTENSITY: f64 = 0.12;

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
            let percentage = (voltage - threshold.0) / (threshold.1 - threshold.0);
            let level = threshold.2 + percentage * (threshold.3 - threshold.2);
            return level;
        }
    }
    0.0
}

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
        // threshold intensity, 12*12mA = 144mA
        let mut v = self.i2c.smbus_read_byte(0x0c)?;
        v &= 0b0000_0111;
        v |= 12 << 3;
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

        let v = (high & 0b0011_1111) + low;
        let v = (v as f64) * 0.26855 + 2600.0;
        Ok(v / 1000.0)
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
        // threshold intensity, 30*4.3mA = 126mA
        let mut v = self.i2c.smbus_read_byte(0xc9)?;
        v &= 0b1100_0000;
        v |= 30;
        self.i2c.smbus_write_byte(0xc9, v)?;

        // time, 8s
        let mut v = self.i2c.smbus_read_byte(0x06)?;
        v &= 0b0011_1111;
        self.i2c.smbus_write_byte(0x07, v)?;

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
        let mut t = self.i2c.smbus_read_byte(0x5B)?;
        t |= 0b0001_0010;
        self.i2c.smbus_write_byte(0x5B, t)?;

        // force shutdown
        t = self.i2c.smbus_read_byte(0x5B)?;
        t &= 0b1110_1111;
        self.i2c.smbus_write_byte(0x5B, t)?;

        Ok(())
    }
}

/// SD3078 time, always 24hr
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SD3078Time([u8; 7]);

impl SD3078Time {
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

impl From<[u8; 7]> for SD3078Time {
    fn from(a: [u8; 7]) -> Self {
        Self(a)
    }
}

impl From<DateTime<Local>> for SD3078Time {
    fn from(dt: DateTime<Local>) -> Self {
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

impl From<SD3078Time> for DateTime<Local> {
    fn from(t: SD3078Time) -> Self {
        let sec = bcd_to_dec(t.0[0]) as u32;
        let min = bcd_to_dec(t.0[1]) as u32;
        let hour = bcd_to_dec(t.0[2]) as u32;
        let day_of_month = bcd_to_dec(t.0[4]) as u32;
        let month = bcd_to_dec(t.0[5]) as u32;
        let year = 2000 + bcd_to_dec(t.0[6]) as i32;

        let datetime = Local.ymd(year, month, day_of_month).and_hms(hour, min, sec);
        datetime
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

    /// Read time
    pub fn read_time(&self) -> Result<SD3078Time> {
        let mut bcd_time = [0_u8; 7];
        self.i2c.block_read(0, &mut bcd_time)?;

        // 12hr or 24hr
        if bcd_time[2] & 0b1000_0000 != 0 {
            bcd_time[2] &= 0b0111_1111; // 24hr
        } else if bcd_time[2] & 0b0010_0000 != 0 {
            bcd_time[2] += 12; // 12hr and pm
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

        // alarm allows hour/minus/second
        self.i2c.smbus_write_byte(0x0e, 0b0000_0111)?;

        self.disable_write()?;

        Ok(())
    }

    /// Set a test wake up after 1 minutes
    pub fn set_test_wake(&self) -> Result<()> {
        let now = Local::now();
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
    if let Ok(_) = execute_shell(cmd.as_str()) {
        let cmd = "/sbin/hwclock -w";
        if let Ok(_) = execute_shell(cmd) {
            return;
        }
    }
    log::error!("Failed to write time to system");
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
    ip5209: IP5209,
    ip5312: IP5312,
    sd3078: SD3078,
    model: String,
    voltage: f64,
    intensity: f64,
    level: f64,
    level_records: VecDeque<f64>,
    updated_at: Instant,
    rtc_time: DateTime<Local>,
    gpio_tap_history: String,
}

impl PiSugarStatus {
    pub fn new() -> Result<Self> {
        let mut level_records = VecDeque::with_capacity(10);

        let mut model = String::from(MODEL_V2);
        let mut voltage = 0.0;
        let mut intensity = 0.0;

        let ip5209 = IP5209::new(I2C_ADDR_BAT)?;
        let ip5312 = IP5312::new(I2C_ADDR_BAT)?;
        let sd3078 = SD3078::new(I2C_ADDR_RTC)?;

        if let Ok(v) = ip5312.read_voltage() {
            log::info!("PiSugar with IP5312");
            model = String::from(MODEL_V2_PRO);
            voltage = v;
            intensity = ip5312.read_intensity().unwrap_or(0.0);

            if ip5312.init_gpio().is_ok() {
                log::info!("Init GPIO success");
            } else {
                log::error!("Init GPIO failed");
            }

            if ip5312.init_auto_shutdown().is_ok() {
                log::info!("Init auto shutdown success");
            } else {
                log::error!("Init auto shutdown failed");
            }
        } else if let Ok(v) = ip5209.read_voltage() {
            log::info!("PiSugar with IP5209");
            model = String::from(MODEL_V2);
            voltage = v;
            intensity = ip5209.read_intensity().unwrap_or(0.0);

            if ip5209.init_gpio().is_ok() {
                log::info!("Init GPIO success");
            } else {
                log::error!("Init GPIO failed");
            }

            if ip5209.init_auto_shutdown().is_ok() {
                log::info!("Init auto shutdown success");
            } else {
                log::error!("Init auto shutdown failed");
            }
        } else {
            log::error!("PiSugar not found");
        }

        let level = convert_battery_voltage_to_level(voltage);
        for _ in 0..level_records.capacity() {
            level_records.push_back(level);
        }

        let rtc_now = match sd3078.read_time() {
            Ok(t) => t.into(),
            Err(_) => Local::now(),
        };

        Ok(Self {
            ip5209,
            ip5312,
            sd3078,
            model,
            voltage,
            intensity,
            level,
            level_records,
            updated_at: Instant::now(),
            rtc_time: rtc_now,
            gpio_tap_history: String::with_capacity(10),
        })
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
            let x_sum = (0.0 + capacity - 1.0) * capacity / 2.0;
            let x_bar = x_sum / capacity;
            let y_sum: f64 = self.level_records.iter().sum();
            let _y_bar = y_sum / capacity;
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

        // gpio tap detect
        if self.mode() == MODEL_V2 {
            if let Ok(t) = self.ip5209.read_gpio_tap() {
                log::debug!("gpio button state: {}", t);
                if t != 0 {
                    self.gpio_tap_history.push('1');
                } else {
                    self.gpio_tap_history.push('0');
                }
            }
        } else {
            if let Ok(t) = self.ip5312.read_gpio_tap() {
                log::debug!("gpio button state: {}", t);
                if t != 0 {
                    self.gpio_tap_history.push('1');
                } else {
                    self.gpio_tap_history.push('0');
                }
            }
        }
        if let Some(tap_type) = gpio_detect_tap(&mut self.gpio_tap_history) {
            log::debug!("tap detected: {}", tap_type);
            return Ok(Some(tap_type));
        }

        // rtc
        if let Ok(rtc_time) = self.sd3078.read_time() {
            self.set_rtc_time(rtc_time.into())
        }

        // others, slower
        if now > self.updated_at && now.duration_since(self.updated_at) > I2C_READ_INTERVAL * 4 {
            // battery
            if self.mode() == MODEL_V2 {
                if let Ok(v) = self.ip5209.read_voltage() {
                    log::debug!("voltage {}", v);
                    self.update_voltage(v, now);
                }
                if let Ok(i) = self.ip5209.read_intensity() {
                    log::debug!("intensity {}", i);
                    self.update_intensity(i, now);
                }
            } else {
                if let Ok(v) = self.ip5312.read_voltage() {
                    log::debug!("voltage {}", v);
                    self.update_voltage(v, now)
                }
                if let Ok(i) = self.ip5312.read_intensity() {
                    log::debug!("intensity {}", i);
                    self.update_intensity(i, now)
                }
            }

            // auto shutdown
            if self.level() < config.auto_shutdown_level {
                loop {
                    log::error!("Low battery, will power off...");
                    let _ = execute_shell("/sbin/shutdown --poweroff 0");
                    thread::sleep(std::time::Duration::from_millis(3000));
                }
            }
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
fn gpio_detect_tap(gpio_history: &mut String) -> Option<TapType> {
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
fn execute_shell(shell: &str) -> io::Result<ExitStatus> {
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
    pub fn new(config: PiSugarConfig) -> Result<Self> {
        let status = PiSugarStatus::new()?;
        Ok(Self {
            config_path: None,
            config,
            status,
        })
    }

    pub fn new_with_path(config_path: &Path, auto_recovery: bool) -> Result<Self> {
        if !config_path.is_file() {
            return Err(Error::Other("Not a file".to_string()));
        }

        match Self::load_config(config_path) {
            Ok(core) => Ok(core),
            Err(_) => {
                if auto_recovery {
                    let config = PiSugarConfig::default();
                    let mut core = Self::new(config)?;
                    core.config_path = Some(config_path.to_string_lossy().to_string());
                    return Ok(core);
                } else {
                    return Err(Error::Other("Not recoverable".to_string()));
                }
            }
        }
    }

    pub fn load_config(path: &Path) -> Result<Self> {
        if path.exists() && path.is_file() {
            let mut config = PiSugarConfig::default();
            if config.load(path).is_ok() {
                let mut core = Self::new(config)?;
                core.config_path = Some(path.to_string_lossy().to_string());
                return Ok(core);
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

    pub fn status(&self) -> &PiSugarStatus {
        &self.status
    }

    pub fn status_mut(&mut self) -> &mut PiSugarStatus {
        &mut self.status
    }

    pub fn model(&self) -> String {
        self.status.model.clone()
    }

    pub fn voltage(&self) -> f64 {
        self.status.voltage()
    }

    pub fn intensity(&self) -> f64 {
        self.status.intensity()
    }

    pub fn level(&self) -> f64 {
        self.status.level()
    }

    pub fn charging(&self) -> bool {
        let now = Instant::now();
        self.status.is_charging(now)
    }

    pub fn read_time(&self) -> DateTime<Local> {
        self.status.rtc_time()
    }

    pub fn read_raw_time(&self) -> SD3078Time {
        match self.status.sd3078.read_time() {
            Ok(t) => t,
            Err(_) => self.status.rtc_time.into(),
        }
    }

    pub fn write_time(&self, dt: DateTime<Local>) -> Result<()> {
        self.status.sd3078.write_time(dt.into())
    }

    pub fn set_alarm(&self, t: SD3078Time, weakday_repeat: u8) -> Result<()> {
        self.status.sd3078.set_alarm(t, weakday_repeat)
    }

    pub fn read_alarm_flag(&self) -> Result<bool> {
        self.status.sd3078.read_alarm_flag()
    }

    pub fn clear_alarm_flag(&self) -> Result<()> {
        self.status.sd3078.clear_alarm_flag()
    }

    pub fn disable_alarm(&self) -> Result<()> {
        self.status.sd3078.disable_alarm()
    }

    pub fn test_wake(&self) -> Result<()> {
        self.status.sd3078.set_test_wake()
    }

    pub fn config(&self) -> &PiSugarConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut PiSugarConfig {
        &mut self.config
    }
}
