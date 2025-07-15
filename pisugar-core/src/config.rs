use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

/// Battery voltage threshold, (low, percentage at low)
pub type BatteryThreshold = (f32, f32);

fn default_i2c_bus() -> u8 {
    1
}

/// Default auth session timeout, 1h
fn default_session_timeout() -> u32 {
    60 * 60
}

/// PiSugar configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct PiSugarConfig {
    /// Http digest auth
    #[serde(default)]
    pub auth_user: Option<String>,

    #[serde(default)]
    pub auth_password: Option<String>,

    /// Auth session timeout in seconds
    #[serde(default = "default_session_timeout")]
    pub session_timeout: u32,

    /// I2C bus, default 1 (/dev/i2c-1)
    #[serde(default = "default_i2c_bus")]
    pub i2c_bus: u8,

    /// I2C addr, default 0x57 (87), available in PiSugar3
    #[serde(default)]
    pub i2c_addr: Option<u16>,

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

    /// Soft poweroff shell script
    #[serde(default)]
    pub soft_poweroff_shell: Option<String>,

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

    /// User defined battery curve
    #[serde(default)]
    pub battery_curve: Option<Vec<BatteryThreshold>>,
}

impl PiSugarConfig {
    fn _validate_battery_curve(cfg: &PiSugarConfig) -> bool {
        let mut curve = cfg.battery_curve.clone().unwrap_or_default();
        curve.sort_by(|x, y| x.0.total_cmp(&y.0));
        for i in 1..curve.len() {
            if curve[i].0 == curve[i - 1].0 || curve[i].1 <= curve[i - 1].1 {
                log::error!("Invalid customized battery curve {:?} {:?}", curve[i - 1], curve[i]);
                return false;
            }
        }
        true
    }

    pub fn load(&mut self, path: &Path) -> io::Result<()> {
        let mut f = File::open(path)?;
        let mut buff = String::new();
        let _ = f.read_to_string(&mut buff)?;
        let config = serde_json::from_str(&buff)?;
        if !PiSugarConfig::_validate_battery_curve(&config) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        *self = config;
        Ok(())
    }

    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        let mut f = options.open(path)?;
        let s = serde_json::to_string_pretty(self)?;
        log::info!("Dump config:\n{}", s);
        f.set_len(0)?;
        f.write_all(s.as_bytes())
    }
}

impl Default for PiSugarConfig {
    fn default() -> Self {
        Self {
            auth_user: Default::default(),
            auth_password: Default::default(),
            session_timeout: default_session_timeout(),
            i2c_bus: default_i2c_bus(),
            i2c_addr: Default::default(),
            auto_wake_time: Default::default(),
            auto_wake_repeat: Default::default(),
            single_tap_enable: Default::default(),
            single_tap_shell: Default::default(),
            double_tap_enable: Default::default(),
            double_tap_shell: Default::default(),
            long_tap_enable: Default::default(),
            long_tap_shell: Default::default(),
            auto_shutdown_level: Default::default(),
            auto_shutdown_delay: Default::default(),
            auto_charging_range: Default::default(),
            full_charge_duration: Default::default(),
            auto_power_on: Default::default(),
            soft_poweroff: Default::default(),
            soft_poweroff_shell: Default::default(),
            auto_rtc_sync: Default::default(),
            adj_comm: Default::default(),
            adj_diff: Default::default(),
            rtc_adj_ppm: Default::default(),
            anti_mistouch: Default::default(),
            bat_protect: Default::default(),
            battery_curve: Default::default(),
        }
    }
}
