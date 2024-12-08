use std::str::FromStr;

use anyhow::anyhow;
use anyhow::Error as AnyError;
use chrono::{DateTime, FixedOffset};
use clap::{builder::PossibleValue, ArgAction, Args, Parser, Subcommand};
use enum_variants_strings::EnumVariantsStrings;

#[derive(Debug, Parser, PartialEq)]
#[command(multicall = true)]
#[clap(rename_all = "snake_case")]
pub enum Cmds {
    #[command(subcommand)]
    Get(GetCmds),

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
        enable: bool,
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
    BatteryLedAmount,
    BatteryPowerPlugged,
    BatteryAllowCharging,
    BatteryChargingRange,
    BatteryCharging,
    BatteryInputProtectEnabled,
    BatteryOutputEnabled,
    FullChargeDuration,
    SystemTime,
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

#[derive(Debug, Args, PartialEq)]
pub struct BoolArg {
    #[arg(action = ArgAction::Set)]
    pub enable: bool,
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
    #[case("set_battery_output true", Cmds::SetBatteryOutput(BoolArg{ enable: true }))]
    #[case("set_battery_output false", Cmds::SetBatteryOutput(BoolArg { enable: false }))]
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
