use std::fmt;
use std::str::FromStr;

use clap::builder::PossibleValue;
use clap::ValueEnum;

use crate::ip5312::IP5312Battery;
use crate::pisugar3::{PiSugar3Battery, PiSugar3RTC, I2C_ADDR_P3};
use crate::rtc::RTC;
use crate::{battery::Battery, I2C_ADDR_BAT};
use crate::{config::PiSugarConfig, ip5209::IP5209Battery};
use crate::{Result, I2C_ADDR_RTC, SD3078};

const PISUGAR_2_4LEDS: &str = "PiSugar 2 (4-LEDs)";
const PISUGAR_2_2LEDS: &str = "PiSugar 2 (2-LEDs)";
const PISUGAR_2_PRO: &str = "PiSugar 2 Pro";
const PISUGAR_3: &str = "PiSugar 3";

/// PiSugar model
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[allow(non_camel_case_types)]
pub enum Model {
    PiSugar_2_4LEDs,
    PiSugar_2_2LEDs,
    PiSugar_2_Pro,
    PiSugar_3,
}

impl Model {
    pub fn led_amount(&self) -> u32 {
        match *self {
            Model::PiSugar_2_4LEDs => 4,
            Model::PiSugar_2_2LEDs => 2,
            Model::PiSugar_2_Pro => 2,
            Model::PiSugar_3 => 4,
        }
    }

    pub fn default_battery_i2c_addr(&self) -> u16 {
        match *self {
            Model::PiSugar_3 => I2C_ADDR_P3,
            _ => I2C_ADDR_BAT,
        }
    }

    pub fn default_rtc_i2c_addr(&self) -> u16 {
        match *self {
            Model::PiSugar_3 => I2C_ADDR_P3,
            _ => I2C_ADDR_RTC,
        }
    }

    pub fn bind(&self, cfg: PiSugarConfig) -> Result<Box<dyn Battery + Send>> {
        log::info!(
            "Binding battery i2c bus={} addr={}",
            cfg.i2c_bus,
            cfg.i2c_addr.unwrap_or(self.default_battery_i2c_addr())
        );
        let b: Box<dyn Battery + Send> = match *self {
            Model::PiSugar_2_4LEDs => Box::new(IP5209Battery::new(cfg, *self)?),
            Model::PiSugar_2_2LEDs => Box::new(IP5209Battery::new(cfg, *self)?),
            Model::PiSugar_2_Pro => Box::new(IP5312Battery::new(cfg, *self)?),
            Model::PiSugar_3 => Box::new(PiSugar3Battery::new(cfg, *self)?),
        };
        Ok(b)
    }

    pub fn rtc(&self, cfg: PiSugarConfig) -> Result<Box<dyn RTC + Send>> {
        log::info!(
            "Binding rtc i2c bus={} addr={}",
            cfg.i2c_bus,
            self.default_rtc_i2c_addr()
        );
        let r: Box<dyn RTC + Send> = match *self {
            Model::PiSugar_3 => Box::new(PiSugar3RTC::new(cfg, *self)?),
            _ => Box::new(SD3078::new(cfg, *self)?),
        };
        Ok(r)
    }
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Model::PiSugar_2_4LEDs => PISUGAR_2_4LEDS,
            Model::PiSugar_2_2LEDs => PISUGAR_2_2LEDS,
            Model::PiSugar_2_Pro => PISUGAR_2_PRO,
            Model::PiSugar_3 => PISUGAR_3,
        };
        write!(f, "{}", s)
    }
}

impl FromStr for Model {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            PISUGAR_2_4LEDS => Ok(Model::PiSugar_2_4LEDs),
            PISUGAR_2_2LEDS => Ok(Model::PiSugar_2_2LEDs),
            PISUGAR_2_PRO => Ok(Model::PiSugar_2_Pro),
            PISUGAR_3 => Ok(Model::PiSugar_3),
            _ => Err(()),
        }
    }
}

impl ValueEnum for Model {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Model::PiSugar_2_4LEDs,
            Model::PiSugar_2_2LEDs,
            Model::PiSugar_2_Pro,
            Model::PiSugar_3,
        ]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            Model::PiSugar_2_4LEDs => PossibleValue::new(PISUGAR_2_4LEDS),
            Model::PiSugar_2_2LEDs => PossibleValue::new(PISUGAR_2_2LEDS),
            Model::PiSugar_2_Pro => PossibleValue::new(PISUGAR_2_PRO),
            Model::PiSugar_3 => PossibleValue::new(PISUGAR_3),
        })
    }
}
