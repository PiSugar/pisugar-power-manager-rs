use std::collections::VecDeque;
use std::convert::{From, TryInto};
use std::fmt;
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{DateTime, Datelike, Local, Timelike};
use rppal::i2c::Error as I2cError;
use serde::export::Result::Err;
use serde::{Deserialize, Serialize};

mod ip5209;
mod ip5312;
mod sd3078;

pub use ip5209::IP5209;
pub use ip5312::IP5312;
pub use sd3078::*;

/// Time host
pub const TIME_HOST: &str = "http://cdn.pisugar.com";

/// I2c poll interval
pub const I2C_READ_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// RTC address, SD3078
const I2C_ADDR_RTC: u16 = 0x32;

/// Battery address, IP5209/IP5312
const I2C_ADDR_BAT: u16 = 0x75;

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
            let percentage = (voltage - threshold.0) / (threshold.1 - threshold.0);
            let level = threshold.2 + percentage * (threshold.3 - threshold.2);
            return level;
        }
    }
    0.0
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
    #[serde(default)]
    pub auto_wake_time: Option<DateTime<Local>>,

    #[serde(default)]
    pub auto_wake_repeat: u8,

    #[serde(default)]
    pub single_tap_enable: bool,

    #[serde(default)]
    pub single_tap_shell: String,

    #[serde(default)]
    pub double_tap_enable: bool,

    #[serde(default)]
    pub double_tap_shell: String,

    #[serde(default)]
    pub long_tap_enable: bool,

    #[serde(default)]
    pub long_tap_shell: String,

    #[serde(default)]
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
        let mut options = OpenOptions::new();
        options.write(true).create(true);
        let mut f = options.open(path)?;
        let s = serde_json::to_string_pretty(self)?;
        log::info!("Dump config:\n{}", s);
        f.set_len(0)?;
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

        // battery level, default 100
        let level = if voltage > 0.0 {
            convert_battery_voltage_to_level(voltage)
        } else {
            100.0
        };
        for _ in 0..level_records.capacity() {
            level_records.push_back(level);
        }

        let rtc_now = match sd3078.read_time() {
            Ok(t) => t.try_into().unwrap_or(Local::now()),
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
            return k >= 0.015;
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

            let script = match tap_type {
                TapType::Single => {
                    if config.single_tap_enable {
                        Some(config.single_tap_shell.as_str())
                    } else {
                        None
                    }
                }
                TapType::Double => {
                    if config.double_tap_enable {
                        Some(config.double_tap_shell.as_str())
                    } else {
                        None
                    }
                }
                TapType::Long => {
                    if config.long_tap_enable {
                        Some(config.long_tap_shell.as_str())
                    } else {
                        None
                    }
                }
            };
            if let Some(script) = script {
                log::debug!("execute script \"{}\"", script);
                match execute_shell(script) {
                    Ok(r) => log::debug!("script ok, code: {:?}", r.code()),
                    Err(e) => log::error!("{}", e),
                }
            }

            return Ok(Some(tap_type));
        }

        // rtc
        if let Ok(rtc_time) = self.sd3078.read_time() {
            self.set_rtc_time(rtc_time.try_into().unwrap_or(Local::now()))
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
            log::debug!("Battery level: {}", self.level());
            if self.level() <= config.auto_shutdown_level {
                loop {
                    log::error!("Low battery, will power off...");
                    let _ = execute_shell("/sbin/shutdown --poweroff 0");
                    thread::sleep(std::time::Duration::from_millis(3000));
                }
            }

            // rtc battery charging
            if (self.sd3078.read_battery_low_flag().ok() == Some(true))
                && (self.sd3078.read_battery_charging_flag().ok() == Some(false))
            {
                log::debug!("Enable rtc charging");
                let _ = self.sd3078.toggle_charging(true);
            } else {
                if (self.sd3078.read_battery_high_flag().ok() == Some(true))
                    && (self.sd3078.read_battery_charging_flag().ok() == Some(true))
                {
                    log::debug!("Disable rtc charging");
                    let _ = self.sd3078.toggle_charging(false);
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

    pub fn new_with_path(config_path: &str, auto_recovery: bool) -> Result<Self> {
        let config_path = PathBuf::from(config_path);
        if config_path.is_dir() {
            return Err(Error::Other("Not a file".to_string()));
        }

        match Self::load_config(config_path.as_path()) {
            Ok(core) => {
                if let Some(datetime) = core.config.auto_wake_time {
                    match core.set_alarm(datetime.into(), core.config.auto_wake_repeat) {
                        Ok(_) => log::info!("Init alarm success"),
                        Err(e) => log::warn!("Init alarm failed: {}", e),
                    }
                }
                Ok(core)
            }
            Err(_) => {
                log::warn!("Load configuration failed, auto recovery...");
                if auto_recovery {
                    let config = PiSugarConfig::default();
                    let mut core = Self::new(config)?;
                    core.config_path = Some(config_path.to_string_lossy().to_string());
                    match core.save_config() {
                        Ok(_) => log::info!("Auto recovery success"),
                        Err(e) => log::warn!("Auto recovery failed: {}", e),
                    }
                    return Ok(core);
                } else {
                    return Err(Error::Other("Not recoverable".to_string()));
                }
            }
        }
    }

    fn load_config(path: &Path) -> Result<Self> {
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
        if let Some(config_path) = &self.config_path {
            let path = Path::new(config_path);
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

    pub fn read_alarm_time(&self) -> Result<SD3078Time> {
        self.status.sd3078.read_alarm_time()
    }

    pub fn read_alarm_enabled(&self) -> Result<bool> {
        self.status.sd3078.read_alarm_enabled()
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
