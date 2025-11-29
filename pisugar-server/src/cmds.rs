use std::convert::TryInto;
use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::anyhow;
use anyhow::Error as AnyError;
use anyhow::Result;
use chrono::Local;
use chrono::SecondsFormat;
use chrono::Utc;
use chrono::{DateTime, FixedOffset};
use clap::{builder::PossibleValue, ArgAction, Args, Parser, Subcommand};
use enum_variants_strings::EnumVariantsStrings;
use tokio::sync::Mutex;

use pisugar_core::get_ntp_datetime;
use pisugar_core::sys_write_time;
use pisugar_core::Error;
use pisugar_core::PiSugarCore;
use pisugar_core::RTCRawTime;

#[derive(Debug, Parser, PartialEq)]
#[command(multicall = true)]
#[clap(rename_all = "snake_case")]
pub enum Cmds {
    #[command(subcommand)]
    Get(GetCmds),

    SetBatteryKeepInput(BoolArg),

    SetBatteryChargingRange {
        #[arg(value_delimiter = ',')]
        range: Vec<f32>,
    },

    SetBatteryInputProtect(BoolArg),

    SetBatteryOutput(BoolArg),

    SetFullChargeDuration {
        seconds: u64,
    },

    SetAllowCharging(BoolArg),

    SetRtcAddr {
        addr: u8,
    },

    RtcClearFlag,

    RtcPi2rtc,

    RtcRtc2pi,

    RtcWeb,

    RtcAlarmSet {
        datetime: DateTime<FixedOffset>,
        weekdays: u8,
    },

    RtcAlarmDisable,

    RtcAdjustPpm {
        ppm: f64,
    },

    SetSafeShutdownLevel {
        level: f64,
    },

    SetSafeShutdownDelay {
        delay: f64,
    },

    RtcTestWake,

    SetButtonEnable {
        mode: ButtonMode,
        enable: BoolValue,
    },

    SetButtonShell {
        mode: ButtonMode,
        shell: Vec<String>,
    },

    SetAutoPowerOn(BoolArg),

    SetAuth {
        username: Option<String>,
        password: Option<String>,
    },

    ForceShutdown,

    SetAntiMistouch(BoolArg),

    SetSoftPoweroff(BoolArg),

    SetSoftPoweroffShell {
        shell: Vec<String>,
    },

    SetInputProtect(BoolArg),
}

impl FromStr for Cmds {
    type Err = AnyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut args = shlex::split(s).ok_or(anyhow!("Invalid args"))?;
        if args.as_slice()[1..].iter().any(|a| a.starts_with("-")) {
            args.insert(1, "--".to_string());
        }
        Ok(Self::try_parse_from(args)?)
    }
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
#[clap(rename_all = "snake_case")]
pub enum GetCmds {
    Version,
    Model,
    FirmwareVersion,
    Battery,
    BatteryI,
    BatteryV,
    BatteryKeepInput,
    BatteryLedAmount,
    BatteryPowerPlugged,
    BatteryAllowCharging,
    BatteryChargingRange,
    BatteryCharging,
    BatteryInputProtectEnabled,
    BatteryOutputEnabled,
    FullChargeDuration,
    SystemTime,
    RtcAddr,
    RtcTime,
    RtcTimeList,
    RtcAlarmFlag,
    RtcAlarmTime,
    RtcAlarmTimeList,
    RtcAlarmEnabled,
    RtcAdjustPpm,
    AlarmRepeat,
    SafeShutdownLevel,
    SafeShutdownDelay,
    ButtonEnable { mode: ButtonMode },
    ButtonShell { mode: ButtonMode },
    AutoPowerOn,
    AuthUsername,
    AntiMistouch,
    SoftPoweroff,
    SoftPoweroffShell,
    Temperature,
    InputProtect,
}

#[derive(Debug, EnumVariantsStrings, PartialEq, Eq, Clone, Copy)]
pub enum ButtonMode {
    Single,
    Double,
    Long,
}

impl clap::ValueEnum for ButtonMode {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Single, Self::Double, Self::Long]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(PossibleValue::new(self.to_str()))
    }
}

#[derive(Debug, Args, PartialEq)]
pub struct BatteryRangeArgs {
    pub min: f32,
    pub max: f32,
}

#[derive(Debug, Args, PartialEq, Clone)]
pub struct BoolArg {
    #[arg(action = ArgAction::Set)]
    pub enable: BoolValue,
}

impl BoolArg {
    const TRUE: Self = Self {
        enable: BoolValue(true),
    };
    const FALSE: Self = Self {
        enable: BoolValue(false),
    };

    pub fn value(&self) -> bool {
        *self == Self::TRUE
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct BoolValue(pub bool);

impl From<String> for BoolValue {
    fn from(value: String) -> Self {
        if let Ok(b) = bool::from_str(&value) {
            return Self(b);
        }
        if let Ok(n) = u32::from_str(&value) {
            return Self(n != 0);
        }
        Self(false)
    }
}

pub struct Response {
    pub cmd: Option<String>,            // command name
    pub extras: Vec<String>,            // extra info
    pub result: Result<Option<String>>, // result
}

impl Response {
    pub fn is_ok(&self) -> bool {
        self.result.is_ok()
    }

    pub fn result_string(&self) -> String {
        match &self.result {
            Ok(Some(res)) => res.clone(),
            Ok(None) => "done".to_string(),
            Err(e) => format!("{}", e),
        }
    }

    pub fn extra_and_result_string(&self) -> String {
        let extra: String = self.extras.join(" ");
        if extra.is_empty() {
            self.result_string()
        } else {
            format!("{} {}", extra, self.result_string())
        }
    }

    pub fn entire_string(&self) -> String {
        match &self.cmd {
            Some(cmd) => format!("{}: {}", cmd, self.extra_and_result_string()),
            None => self.extra_and_result_string(),
        }
    }
}

impl Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.entire_string())
    }
}

/// Handle request of cmd
pub async fn handle_request(core: Arc<Mutex<PiSugarCore>>, req: &str) -> Response {
    let parts: Vec<String> = req.split(' ').map(|s| s.to_string()).collect();
    let err = "Invalid request.".to_string();

    if !req.contains("set_auth") {
        log::debug!("Request: {}", req);
    }

    if req.starts_with("help") {
        let help = Cmds::from_str(req).expect_err("");
        return Response {
            cmd: None,
            extras: vec![],
            result: Ok(Some(format!("{}", help))),
        };
    }

    let cmd = match Cmds::from_str(req) {
        Ok(cmd) => cmd,
        Err(e) => {
            log::warn!("Invalid cmd: {}", e);
            return Response {
                cmd: None,
                extras: vec![],
                result: Err(anyhow!(err)),
            };
        }
    };

    let core_cloned = core.clone();
    let mut core = core.lock().await;
    match &cmd {
        // Get commands
        Cmds::Get(get_cmd) => {
            let (extra, r) = match get_cmd {
                GetCmds::ButtonShell { mode } => (
                    vec![parts[2].to_string()],
                    Ok(match mode {
                        ButtonMode::Single => core.config().single_tap_shell.clone(),
                        ButtonMode::Double => core.config().double_tap_shell.clone(),
                        ButtonMode::Long => core.config().long_tap_shell.clone(),
                    }),
                ),
                get_cmd => {
                    let r = match get_cmd {
                        GetCmds::Version => Ok(env!("CARGO_PKG_VERSION").to_string()),
                        GetCmds::Model => Ok(core.model()),
                        GetCmds::FirmwareVersion => core.version(),
                        GetCmds::Battery => core.level().map(|l| l.to_string()),
                        GetCmds::BatteryI => core.intensity_avg().map(|i| i.to_string()),
                        GetCmds::BatteryV => core.voltage_avg().map(|v| v.to_string()),
                        GetCmds::BatteryKeepInput => core.keep_input().map(|k| k.to_string()),
                        GetCmds::BatteryLedAmount => core.led_amount().map(|n| n.to_string()),
                        GetCmds::BatteryPowerPlugged => core.power_plugged().map(|p| p.to_string()),
                        GetCmds::BatteryAllowCharging => core.allow_charging().map(|a| a.to_string()),
                        GetCmds::BatteryChargingRange => core
                            .charging_range()
                            .map(|r| r.map_or("".to_string(), |r| format!("{},{}", r.0, r.1))),
                        GetCmds::BatteryCharging => core.charging().map(|c| c.to_string()),
                        GetCmds::BatteryInputProtectEnabled => core.input_protected().map(|c| c.to_string()),
                        GetCmds::BatteryOutputEnabled => core.output_enabled().map(|o| o.to_string()),
                        GetCmds::FullChargeDuration => Ok(core
                            .config()
                            .full_charge_duration
                            .map_or("".to_string(), |d| d.to_string())),
                        GetCmds::SystemTime => Ok(Local::now().to_rfc3339_opts(SecondsFormat::Millis, false)),
                        GetCmds::RtcAddr => core.read_rtc_addr().map(|a| format!("0x{:02x}", a)),
                        GetCmds::RtcTime => core
                            .read_time()
                            .map(|t| t.to_rfc3339_opts(SecondsFormat::Millis, false)),
                        GetCmds::RtcTimeList => core.read_raw_time().map(|r| r.to_string()),
                        GetCmds::RtcAlarmFlag => core.read_alarm_flag().map(|f| f.to_string()),
                        GetCmds::RtcAlarmTime => {
                            let t = core
                                .read_alarm_time()
                                .and_then(|r| r.try_into().map_err(|_| Error::Other("Invalid".to_string())));
                            t.map(|t: DateTime<Utc>| {
                                t.with_timezone(Local::now().offset())
                                    .to_rfc3339_opts(SecondsFormat::Millis, false)
                            })
                        }
                        GetCmds::RtcAlarmTimeList => core.read_alarm_time().map(|r| r.to_string()),
                        GetCmds::RtcAlarmEnabled => core.read_alarm_enabled().map(|e| e.to_string()),
                        GetCmds::RtcAdjustPpm => Ok(core.config().rtc_adj_ppm.unwrap_or_default().to_string()),
                        GetCmds::AlarmRepeat => Ok(core.config().auto_wake_repeat.to_string()),
                        GetCmds::SafeShutdownLevel => Ok(core.config().auto_shutdown_level.unwrap_or(0.0).to_string()),
                        GetCmds::SafeShutdownDelay => Ok(core.config().auto_shutdown_delay.unwrap_or(0.0).to_string()),
                        GetCmds::ButtonEnable { mode } => Ok(match mode {
                            ButtonMode::Single => core.config().single_tap_enable,
                            ButtonMode::Double => core.config().double_tap_enable,
                            ButtonMode::Long => core.config().long_tap_enable,
                        })
                        .map(|b| format!("{} {}", parts[2], b)),
                        GetCmds::AutoPowerOn => Ok(core.config().auto_power_on.unwrap_or(false).to_string()),
                        GetCmds::AuthUsername => Ok(core.config().auth_user.clone().unwrap_or_default()),
                        GetCmds::AntiMistouch => Ok(core.config().anti_mistouch.unwrap_or(true).to_string()),
                        GetCmds::SoftPoweroff => Ok(core.config().soft_poweroff.unwrap_or(false).to_string()),
                        GetCmds::SoftPoweroffShell => Ok(core.config().soft_poweroff_shell.clone().unwrap_or_default()),
                        GetCmds::Temperature => core.get_temperature().map(|x| x.to_string()),
                        GetCmds::InputProtect => core.input_protected().map(|x| x.to_string()),
                        GetCmds::ButtonShell { mode } => unreachable!(),
                    };
                    (vec![], r)
                }
            };
            return Response {
                cmd: Some(parts[1].to_string()),
                extras: extra,
                result: r.map(Some).map_err(|e| anyhow!(e)),
            };
        }
        // Other set commands
        cmd => {
            let r = match cmd {
                Cmds::Get(_) => unreachable!(),
                Cmds::SetBatteryKeepInput(b) => core.set_keep_input(b.value()).map(|_| None),
                Cmds::SetBatteryChargingRange { range } => {
                    let charging_range = if range.len() == 2 {
                        Some((range[0], range[1]))
                    } else {
                        None
                    };
                    core.set_charging_range(charging_range).map(|_| None)
                }
                Cmds::SetBatteryInputProtect(b) => core.toggle_input_protected(b.value()).map(|_| None),
                Cmds::SetBatteryOutput(b) => core.toggle_output_enabled(b.value()).map(|_| None),
                Cmds::SetFullChargeDuration { seconds } => {
                    core.config_mut().full_charge_duration = Some(*seconds);
                    core.save_config().map(|_| None)
                }
                Cmds::SetAllowCharging(b) => core.toggle_allow_charging(b.value()).map(|_| None),
                Cmds::SetRtcAddr { addr } => {
                    if let Err(e) = core.set_rtc_addr(*addr) {
                        log::warn!("Set RTC addr error: {}", e);
                    }
                    Ok(None)
                }
                Cmds::RtcClearFlag => core.clear_alarm_flag().map(|_| None),
                Cmds::RtcPi2rtc => core.write_time(Local::now()).map(|_| None),
                Cmds::RtcRtc2pi => core
                    .read_time()
                    .map(|t| {
                        sys_write_time(t);
                        Ok(None)
                    })
                    .flatten(),
                Cmds::RtcWeb => {
                    tokio::spawn(async move {
                        match get_ntp_datetime().await {
                            Ok(ntp_datetime) => {
                                let core = core_cloned.lock().await;
                                sys_write_time(ntp_datetime.into());
                                if let Err(e) = core.write_time(ntp_datetime.into()) {
                                    log::warn!("Write RTC time error: {}", e);
                                }
                            }
                            Err(e) => log::warn!("Sync NTP time error: {}", e),
                        }
                    });
                    Ok(None)
                }
                Cmds::RtcAlarmSet { datetime, weekdays } => {
                    let datetime: DateTime<Local> = (*datetime).into();
                    let sd3078_time: RTCRawTime = datetime.into();
                    core.write_alarm(sd3078_time, *weekdays).map(|_| {
                        core.config_mut().auto_wake_repeat = *weekdays;
                        core.config_mut().auto_wake_time = Some(datetime);
                        if let Err(e) = core.save_config() {
                            log::warn!("{}", e);
                        }
                        Ok(None)
                    })
                }
                .flatten(),
                Cmds::RtcAlarmDisable => core
                    .disable_alarm()
                    .map(|_| {
                        core.config_mut().auto_wake_time = None;
                        if let Err(e) = core.save_config() {
                            log::warn!("{}", e);
                        }
                        Ok(None)
                    })
                    .flatten(),
                Cmds::RtcAdjustPpm { ppm } => {
                    let ppm = if *ppm > 500.0 { 500.0 } else { *ppm };
                    let ppm = if ppm < -500.0 { -500.0 } else { ppm };
                    core.write_rtc_adjust_ppm(ppm).map(|_| {
                        core.config_mut().rtc_adj_ppm = Some(ppm);
                        if let Err(e) = core.save_config() {
                            log::warn!("{}", e);
                        }
                        Ok(None)
                    })
                }
                .flatten(),
                Cmds::SetSafeShutdownLevel { level } => {
                    // level between <30，level < 0 means do not shutdown
                    let level = if *level > 30.0 { 30.0 } else { *level };
                    core.config_mut().auto_shutdown_level = Some(level);
                    if let Err(e) = core.save_config() {
                        log::error!("{}", e);
                    }
                    Ok(None)
                }
                Cmds::SetSafeShutdownDelay { delay } => {
                    // delay between 0-30
                    let delay = if *delay < 0.0 { 0.0 } else { *delay };
                    let delay = if delay > 120.0 { 120.0 } else { delay };
                    core.config_mut().auto_shutdown_delay = Some(delay);
                    if let Err(e) = core.save_config() {
                        log::error!("{}", e);
                    }
                    Ok(None)
                }
                Cmds::RtcTestWake => core
                    .test_wake()
                    .map(|_| Some(format!("{}: wakeup after 1 min 30 sec", parts[0]))),
                Cmds::SetButtonEnable { mode, enable } => {
                    match *mode {
                        ButtonMode::Single => core.config_mut().single_tap_enable = enable.0,
                        ButtonMode::Double => core.config_mut().double_tap_enable = enable.0,
                        ButtonMode::Long => core.config_mut().long_tap_enable = enable.0,
                    }
                    if let Err(e) = core.save_config() {
                        log::error!("{}", e);
                    }
                    Ok(None)
                }
                Cmds::SetButtonShell { mode, shell } => {
                    let cmd = shell.join(" ");
                    match mode {
                        ButtonMode::Single => core.config_mut().single_tap_shell = cmd,
                        ButtonMode::Double => core.config_mut().double_tap_shell = cmd,
                        ButtonMode::Long => core.config_mut().long_tap_shell = cmd,
                    }
                    if let Err(e) = core.save_config() {
                        log::error!("{}", e);
                    }
                    Ok(None)
                }
                Cmds::SetAutoPowerOn(b) => core.toggle_auto_power_on(b.value()).map(|_| None),
                Cmds::SetAuth { username, password } => {
                    if let (Some(username), Some(password)) = (username, password) {
                        core.config_mut().auth_user = Some(username.to_string());
                        core.config_mut().auth_password = Some(password.to_string());
                    } else {
                        core.config_mut().auth_user = None;
                        core.config_mut().auth_password = None;
                    }
                    core.save_config().map(|_| None)
                }
                Cmds::ForceShutdown => core.force_shutdown().map(|_| None),
                Cmds::SetAntiMistouch(b) => core.toggle_anti_mistouch(b.value()).map(|_| None),
                Cmds::SetSoftPoweroff(b) => core.toggle_soft_poweroff(b.value()).map(|_| None),
                Cmds::SetSoftPoweroffShell { shell } => {
                    let script = shell.join(" ");
                    core.config_mut().soft_poweroff_shell = if !script.is_empty() {
                        Some(script.to_string())
                    } else {
                        None
                    };
                    core.save_config().map(|_| None)
                }
                Cmds::SetInputProtect(b) => core.toggle_input_protected(b.value()).map(|_| None),
            };
            return match r {
                Ok(r) => Response {
                    cmd: Some(parts[0].to_string()),
                    extras: vec![],
                    result: Ok(r),
                },
                Err(e) => {
                    log::warn!("Request: {}, error: {}", req, e);
                    Response {
                        cmd: Some(parts[0].to_string()),
                        extras: vec![],
                        result: Err(anyhow!(e)),
                    }
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {

    use anyhow::Result;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("get version", Cmds::Get(GetCmds::Version))]
    #[case("get model", Cmds::Get(GetCmds::Model))]
    #[case("get firmware_version", Cmds::Get(GetCmds::FirmwareVersion))]
    #[case("get button_enable single", Cmds::Get(GetCmds::ButtonEnable{ mode: ButtonMode::Single }))]
    #[case("get button_shell long", Cmds::Get(GetCmds::ButtonShell { mode: ButtonMode::Long } ))]
    #[case("set_battery_charging_range 30.0,80.0", Cmds::SetBatteryChargingRange{ range: vec![30.0, 80.0]})]
    #[case("set_battery_output true", Cmds::SetBatteryOutput(BoolArg::TRUE))]
    #[case("set_battery_output false", Cmds::SetBatteryOutput(BoolArg::FALSE))]
    #[case("set_button_enable single 1", Cmds::SetButtonEnable { mode: ButtonMode::Single, enable: BoolValue(true) })]
    #[case("set_button_shell single echo hello", Cmds::SetButtonShell { mode: ButtonMode::Single, shell: vec!["echo".to_string(), "hello".to_string()] })]
    #[case("set_soft_poweroff_shell shutdown -a", Cmds::SetSoftPoweroffShell { shell: vec!["shutdown".to_string(), "-a".to_string()] })]
    #[case("set_soft_poweroff_shell bash \"shutdown -a\"", Cmds::SetSoftPoweroffShell { shell: vec!["bash".to_string(), "shutdown -a".to_string()] })]
    fn test_cmds(#[case] repl: &str, #[case] cmd: Cmds) -> Result<()> {
        assert!(cmd == Cmds::from_str(repl)?);
        Ok(())
    }

    #[rstest]
    fn test_help() {
        let h = Cmds::from_str("help");
        assert!(format!("{:?}", h).contains("help"));
    }

    #[rstest]
    fn test_help_get() {
        let h = Cmds::from_str("help get");
        println!("{:?}", h);
        assert!(format!("{:?}", h).contains("version"));
    }
}
