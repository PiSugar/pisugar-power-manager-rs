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
use hyper::client::Client;
use rppal::i2c::Error as I2cError;
use serde::{Deserialize, Serialize};

pub use model::Model;
pub use sd3078::*;

use crate::battery::Battery;
pub use crate::rtc::RTCRawTime;
use crate::rtc::RTC;

mod battery;
mod ip5209;
mod ip5312;
mod model;
mod pisugar3;
mod rtc;
mod sd3078;

/// Time host
pub const TIME_HOST: &str = "http://cdn.pisugar.com";

/// RTC Time record
pub const RTC_TIME: &str = "rtc.time";

/// I2c poll interval, no more than 1s
pub const I2C_READ_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// RTC address, SD3078
const I2C_ADDR_RTC: u16 = 0x32;

/// Battery address, IP5209/IP5312
const I2C_ADDR_BAT: u16 = 0x75;

/// Battery full charge 5min after full, 5min, should be adjust as needed
const BAT_FULL_CHARGE_DURATION: u64 = 5 * 60;

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

/// Battery voltage threshold, (low, percentage at low)
type BatteryThreshold = (f32, f32);

/// Battery voltage to percentage level
fn convert_battery_voltage_to_level(voltage: f32, battery_curve: &[BatteryThreshold]) -> f32 {
    for i in 0..battery_curve.len() {
        let v_low = battery_curve[i].0;
        let l_low = battery_curve[i].1;
        if voltage >= v_low {
            if i == 0 {
                return l_low;
            } else {
                let v_high = battery_curve[i - 1].0;
                let l_high = battery_curve[i - 1].1;
                let percent = (voltage - v_low) / (v_high - v_low);
                return l_low + percent * (l_high - l_low);
            }
        }
    }
    0.0
}

/// Write time to system
pub fn sys_write_time(dt: DateTime<Local>) {
    let cmd = format!(
        "/bin/date -s \"{}-{}-{} {}:{}:{}\"",
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
        } else {
            log::warn!("Failed to set RTC");
        }
    } else {
        log::error!("Failed to set system time");
    }
}

fn default_i2c_bus() -> u8 {
    1
}

/// PiSugar configuration
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct PiSugarConfig {
    /// Http digest auth
    #[serde(default)]
    pub digest_auth: Option<(String, String)>,

    /// I2C bus, default 1 (/dev/i2c-1)
    #[serde(default = "default_i2c_bus")]
    pub i2c_bus: u8,

    /// I2C addr, default 0x57 (87), available in PiSugar3
    #[serde(default)]
    pub i2c_addr: Option<u8>,

    /// Alarm time
    #[serde(default)]
    pub auto_wake_time: Option<DateTime<Local>>,

    /// Alarm weekday repeat
    #[serde(default)]
    pub auto_wake_repeat: u8,

    /// Single tap enable
    #[serde(default)]
    pub single_tap_enable: bool,

    /// Single tap shell script
    #[serde(default)]
    pub single_tap_shell: String,

    /// Double tap enable
    #[serde(default)]
    pub double_tap_enable: bool,

    /// Double tap shell script
    #[serde(default)]
    pub double_tap_shell: String,

    /// Long tap enable
    #[serde(default)]
    pub long_tap_enable: bool,

    /// Long tap shell script
    #[serde(default)]
    pub long_tap_shell: String,

    /// Auto shutdown when battery level is low
    #[serde(default)]
    pub auto_shutdown_level: Option<f64>,

    /// Auto shutdown delay, seconds
    #[serde(default)]
    pub auto_shutdown_delay: Option<f64>,

    /// Charging range
    #[serde(default)]
    pub auto_charging_range: Option<(f32, f32)>,

    /// Keep charging duration
    #[serde(default)]
    pub full_charge_duration: Option<u64>,

    /// UPS automatically power on when power recovered
    #[serde(default)]
    pub auto_power_on: Option<bool>,

    /// Soft poweroff, PiSugar 3 only
    #[serde(default)]
    pub soft_poweroff: Option<bool>,

    /// Auto rtc sync
    #[serde(default)]
    pub auto_rtc_sync: Option<bool>,

    /// RTC ppm adjust comm (every second)
    #[serde(default)]
    pub adj_comm: Option<u8>,

    /// RTC ppm adjust diff (in 31s)
    #[serde(default)]
    pub adj_diff: Option<u8>,

    /// RTC adjust ppm
    #[serde(default)]
    pub rtc_adj_ppm: Option<f64>,

    /// Anti mistouch
    #[serde(default)]
    pub anti_mistouch: Option<bool>,

    /// Battery hardware protect
    #[serde(default)]
    pub bat_protect: Option<bool>,
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

/// Notify shutdown with message
pub fn notify_shutdown_soon(message: &str) {
    let shell = format!("/usr/bin/wall -n '{}'", message);
    let _ = execute_shell(shell.as_str());
}

macro_rules! call_i2c {
    ($i2c_addr:expr, $obj:expr, $method:tt) => {
        if let Some(obj) = $obj {
            obj.$method()
        } else {
            Err(I2cError::InvalidSlaveAddress($i2c_addr).into())
        }
    };
    ($i2c_addr:expr, $obj:expr, $method:tt, $($arg:tt)*) => {
        if let Some(obj) = $obj {
            obj.$method($($arg)*)
        } else {
            Err(I2cError::InvalidSlaveAddress($i2c_addr).into())
        }
    };
}

macro_rules! call_battery {
    ($battery:expr, $method:tt) => {
        call_i2c!(I2C_ADDR_BAT, $battery, $method)
    };
    ($battery:expr, $method:tt, $($arg:tt)*) => {
        call_i2c!(I2C_ADDR_BAT, $battery, $method, $($arg)*)
    }
}

macro_rules! call_rtc {
    ($rtc:expr, $method:tt) => {
        call_i2c!(I2C_ADDR_RTC, $rtc, $method)
    };
    ($rtc:expr, $method:tt, $($arg:tt)*) => {
        call_i2c!(I2C_ADDR_RTC, $rtc, $method, $($arg)*)
    }
}

/// Core
pub struct PiSugarCore {
    config_path: Option<String>,
    config: PiSugarConfig,
    model: Model,
    battery: Option<Box<dyn Battery + Send>>,
    battery_full_at: Option<Instant>,
    rtc: Option<Box<dyn RTC + Send>>,
    poll_check_at: Instant,
    rtc_sync_at: Instant,
    ready: bool,
}

impl PiSugarCore {
    fn init_battery(&mut self) -> Result<()> {
        if self.battery.is_none() {
            let i2c_addr_bat = match self.model {
                Model::PiSugar_3 => self
                    .config
                    .i2c_addr
                    .map(|addr| addr as u16)
                    .unwrap_or(pisugar3::I2C_ADDR_P3),
                _ => I2C_ADDR_BAT,
            };
            log::debug!("Battery i2c addr: {:02x}({})", i2c_addr_bat, self.model);
            let battery = self.model.bind(self.config.i2c_bus, i2c_addr_bat)?;
            self.battery = Some(battery);
        }
        Ok(())
    }

    fn init_rtc(&mut self) -> Result<()> {
        if self.rtc.is_none() {
            let rtc = self.model.rtc(self.config.i2c_bus)?;
            self.rtc = Some(rtc);
        }
        Ok(())
    }

    pub fn new(config: PiSugarConfig, model: Model) -> Result<Self> {
        let mut core = Self {
            config_path: None,
            config,
            model,
            battery: None,
            battery_full_at: None,
            rtc: None,
            poll_check_at: Instant::now(),
            rtc_sync_at: Instant::now(),
            ready: false,
        };
        let _ = core.init_rtc();
        let _ = core.init_battery();
        Ok(core)
    }

    pub fn new_with_path(config_path: &str, recover_config: bool, model: Model) -> Result<Self> {
        let config_path = PathBuf::from(config_path);
        if config_path.is_dir() {
            return Err(Error::Other("Not a file".to_string()));
        }

        match Self::load_config(config_path.as_path(), model) {
            Ok(core) => Ok(core),
            Err(_) => {
                log::warn!("Load configuration failed, auto recovery...");
                if recover_config {
                    let config = PiSugarConfig::default();
                    let mut core = Self::new(config, model)?;
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

    fn load_config(path: &Path, model: Model) -> Result<Self> {
        if path.exists() && path.is_file() {
            let mut config = PiSugarConfig::default();
            if config.load(path).is_ok() {
                let mut core = Self::new(config, model)?;
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

    pub fn model(&self) -> String {
        self.model.to_string()
    }

    pub fn led_amount(&self) -> Result<u32> {
        Ok(self.model.led_amount())
    }

    pub fn version(&self) -> Result<String> {
        call_battery!(&self.battery, version)
    }

    pub fn voltage(&self) -> Result<f32> {
        call_battery!(&self.battery, voltage)
    }

    pub fn voltage_avg(&self) -> Result<f32> {
        call_battery!(&self.battery, voltage_avg)
    }

    pub fn intensity(&self) -> Result<f32> {
        call_battery!(&self.battery, intensity)
    }

    pub fn intensity_avg(&self) -> Result<f32> {
        call_battery!(&self.battery, intensity_avg)
    }

    pub fn level(&self) -> Result<f32> {
        call_battery!(&self.battery, level)
    }

    pub fn power_plugged(&self) -> Result<bool> {
        call_battery!(&self.battery, is_power_plugged)
    }

    pub fn allow_charging(&self) -> Result<bool> {
        call_battery!(&self.battery, is_allow_charging)
    }

    pub fn toggle_allow_charging(&self, enable: bool) -> Result<()> {
        call_battery!(&self.battery, toggle_allow_charging, enable)
    }

    pub fn charging(&self) -> Result<bool> {
        call_battery!(&self.battery, is_charging)
    }

    pub fn input_protected(&self) -> Result<bool> {
        call_battery!(&self.battery, is_input_protected)
    }

    pub fn toggle_input_protected(&self, enable: bool) -> Result<()> {
        call_battery!(&self.battery, toggle_input_protected, enable)
    }

    pub fn output_enabled(&self) -> Result<bool> {
        call_battery!(&self.battery, output_enabled)
    }

    pub fn toggle_output_enabled(&self, enable: bool) -> Result<()> {
        call_battery!(&self.battery, toggle_output_enabled, enable)
    }

    pub fn charging_range(&self) -> Result<Option<(f32, f32)>> {
        Ok(self.config.auto_charging_range)
    }

    pub fn set_charging_range(&mut self, range: Option<(f32, f32)>) -> Result<()> {
        if let Some((begin, end)) = range {
            if begin < 0.0 || end < begin || end > 100.0 {
                return Err(Error::Other("Invalid charging range".to_string()));
            }
        } else {
            self.toggle_allow_charging(true)?;
        }
        self.config.auto_charging_range = range;
        self.save_config()
    }

    pub fn read_time(&self) -> Result<DateTime<Local>> {
        call_rtc!(&self.rtc, read_time)
            .and_then(|t| t.try_into().map_err(|_| Error::Other("Invalid datetime".to_string())))
    }

    pub fn read_raw_time(&self) -> Result<RTCRawTime> {
        call_rtc!(&self.rtc, read_time)
    }

    pub fn write_time(&self, dt: DateTime<Local>) -> Result<()> {
        call_rtc!(&self.rtc, write_time, dt.into())
    }

    pub fn write_alarm(&self, t: RTCRawTime, weekday_repeat: u8) -> Result<()> {
        if self.config.auto_power_on == Some(true) {
            return Err(Error::Other(
                "auto_power_on is in conflict with alarm function".to_string(),
            ));
        }
        call_rtc!(&self.rtc, set_alarm, t, weekday_repeat)
    }

    pub fn read_alarm_time(&self) -> Result<RTCRawTime> {
        call_rtc!(&self.rtc, read_alarm_time)
    }

    pub fn read_alarm_enabled(&self) -> Result<bool> {
        call_rtc!(&self.rtc, is_alarm_enable)
    }

    pub fn write_rtc_adjust_ppm(&self, ppm: f64) -> Result<()> {
        call_rtc!(&self.rtc, write_adjust_ppm, ppm)
    }

    pub fn read_alarm_flag(&self) -> Result<bool> {
        call_rtc!(&self.rtc, read_alarm_flag)
    }

    pub fn clear_alarm_flag(&self) -> Result<()> {
        call_rtc!(&self.rtc, clear_alarm_flag)
    }

    pub fn disable_alarm(&self) -> Result<()> {
        call_rtc!(&self.rtc, toggle_alarm_enable, false)
    }

    pub fn toggle_auto_power_on(&mut self, auto_power_on: bool) -> Result<()> {
        self.config.auto_power_on = Some(auto_power_on);
        self.save_config()?;

        match self.model {
            Model::PiSugar_3 => {
                call_battery!(&self.battery, toggle_power_restore, auto_power_on)?;
            }
            _ => {
                // NOTE: pisugar 2 use frequency alarm to restore power
                if auto_power_on {
                    call_rtc!(&self.rtc, toggle_frequency_alarm, true)?;
                } else {
                    call_rtc!(&self.rtc, toggle_frequency_alarm, false)?;
                    // restore clock alarm
                    if let Some(wakeup_time) = self.config.auto_wake_time {
                        self.write_alarm(wakeup_time.into(), self.config.auto_wake_repeat)?;
                    }
                }
                call_battery!(&self.battery, toggle_light_load_shutdown, !auto_power_on)?;
            }
        }

        Ok(())
    }

    pub fn toggle_anti_mistouch(&mut self, anti_mistouch: bool) -> Result<()> {
        self.config.anti_mistouch = Some(anti_mistouch);
        self.save_config()?;
        call_battery!(&self.battery, toggle_anti_mistouch, anti_mistouch)
    }

    pub fn toggle_soft_poweroff(&mut self, soft_poweroff: bool) -> Result<()> {
        self.config.soft_poweroff = Some(soft_poweroff);
        self.save_config()?;
        call_battery!(&self.battery, toggle_soft_poweroff, soft_poweroff)
    }

    pub fn get_temperature(&self) -> Result<f32> {
        call_battery!(&self.battery, temperature)
    }

    pub fn test_wake(&self) -> Result<()> {
        call_rtc!(&self.rtc, set_test_wake)
    }

    pub fn config(&self) -> &PiSugarConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut PiSugarConfig {
        &mut self.config
    }

    pub fn force_shutdown(&self) -> Result<()> {
        // exec 30 sync before shutdown
        for _ in 0..30 {
            let _ = execute_shell("sync");
        }

        let _ = call_rtc!(&self.rtc, force_shutdown);
        call_battery!(&self.battery, shutdown)
    }

    pub async fn poll(&mut self, now: Instant) -> Result<Option<TapType>> {
        if !self.ready {
            log::info!("Init battery...");
            call_battery!(&mut self.battery, init, &self.config)?;
            log::info!("Init rtc...");
            call_rtc!(&mut self.rtc, init, &self.config)?;
            self.ready = true;
        }

        // tap
        let config = &self.config;
        let tap = call_battery!(&mut self.battery, poll, now, config)?;
        if let Some(tap_type) = tap {
            let script = match tap_type {
                TapType::Single => {
                    if config.single_tap_enable {
                        Some(config.single_tap_shell.clone())
                    } else {
                        None
                    }
                }
                TapType::Double => {
                    if config.double_tap_enable {
                        Some(config.double_tap_shell.clone())
                    } else {
                        None
                    }
                }
                TapType::Long => {
                    if config.long_tap_enable {
                        Some(config.long_tap_shell.clone())
                    } else {
                        None
                    }
                }
            };
            if let Some(script) = script {
                log::debug!("Execute script \"{}\"", script);
                thread::spawn(move || match execute_shell(script.as_str()) {
                    Ok(r) => log::debug!("Script ok, code: {:?}", r.code()),
                    Err(e) => log::error!("{}", e),
                });
            }
        }

        // slower
        if self.poll_check_at + Duration::from_secs(1) <= now {
            log::debug!("Poll check");
            self.poll_check_at = now;

            // 2-led, auto allow charging
            if self.model != Model::PiSugar_3 && self.led_amount().unwrap_or(4) == 2 {
                if let Some((begin, end)) = &self.config.auto_charging_range {
                    let l = self.level().unwrap_or(0.0);
                    let allow_charging = self.allow_charging().unwrap_or(false);
                    if l < *begin && !allow_charging {
                        self.battery_full_at = None;
                        let is_ok = self.toggle_allow_charging(true).map_or("fail", |_| "ok");
                        log::info!("Battery {} <= {}, enable charging: {}", l, *begin, is_ok);
                    }
                    if (l >= *end && allow_charging) || l >= 99.9 {
                        let should_stop = if self.battery_full_at.is_none() {
                            log::info!("Battery {} >= {}, full", l, *end);
                            self.battery_full_at = Some(now);
                            false
                        } else {
                            let full_at = self.battery_full_at.unwrap();
                            let delay = Duration::from_secs(
                                self.config.full_charge_duration.unwrap_or(BAT_FULL_CHARGE_DURATION),
                            );
                            now.duration_since(full_at) > delay
                        };
                        if should_stop {
                            let is_ok = self.toggle_allow_charging(false).map_or("fail", |_| "ok");
                            log::info!("Battery {} >= {}, stop charging: {}", l, *end, is_ok);
                        }
                    }
                }
            }

            // rtc battery charging
            if let Some(rtc) = &self.rtc {
                if rtc.read_battery_low_flag().ok() == Some(true) {
                    log::debug!("Enable rtc charging");
                    let _ = rtc.toggle_charging(true);
                } else {
                    if rtc.read_battery_high_flag().ok() == Some(true) {
                        log::debug!("Disable rtc charging");
                        let _ = rtc.toggle_charging(false);
                    }
                }
            }
        }

        // much slower
        if self.config.auto_rtc_sync == Some(true) && self.rtc_sync_at + Duration::from_secs(10) <= now {
            self.rtc_sync_at = now;
            if let Ok(resp) = Client::new().get(TIME_HOST.parse().unwrap()).await {
                if let Some(date) = resp.headers().get("Date") {
                    if let Ok(s) = date.to_str() {
                        if let Ok(dt) = DateTime::parse_from_rfc2822(s) {
                            sys_write_time(dt.into());
                            let _ = self.write_time(dt.into());
                        }
                    }
                }
            }
        }

        Ok(tap)
    }
}

// Fix aarch64

#[cfg(test)]
mod tests {
    use super::PiSugarConfig;

    #[test]
    fn test_config() {
        let config = PiSugarConfig::default();
        assert!(serde_json::to_string(&config).is_ok())
    }
}
