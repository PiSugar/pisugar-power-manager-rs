use std::convert::TryFrom;
use std::fmt;

use crate::battery::Battery;
use crate::ip5209::IP5209Battery;
use crate::ip5312::IP5312Battery;
use crate::pisugar3::{PiSugar3Battery, PiSugar3RTC, I2C_ADDR_P3};
use crate::rtc::RTC;
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

    pub fn bind(&self, i2c_bus: u8, i2c_addr: u16) -> Result<Box<dyn Battery + Send>> {
        let b: Box<dyn Battery + Send> = match *self {
            Model::PiSugar_2_4LEDs => Box::new(IP5209Battery::new(i2c_bus, i2c_addr, *self)?),
            Model::PiSugar_2_2LEDs => Box::new(IP5209Battery::new(i2c_bus, i2c_addr, *self)?),
            Model::PiSugar_2_Pro => Box::new(IP5312Battery::new(i2c_bus, i2c_addr, *self)?),
            Model::PiSugar_3 => Box::new(PiSugar3Battery::new(i2c_bus, i2c_addr, *self)?),
        };
        Ok(b)
    }

    pub fn rtc(&self, i2c_bus: u8) -> Result<Box<dyn RTC + Send>> {
        let r: Box<dyn RTC + Send> = match *self {
            Model::PiSugar_3 => Box::new(PiSugar3RTC::new(i2c_bus, I2C_ADDR_P3)?),
            _ => Box::new(SD3078::new(i2c_bus, I2C_ADDR_RTC)?),
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

impl TryFrom<&str> for Model {
    type Error = ();

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        match value {
            PISUGAR_2_4LEDS => Ok(Model::PiSugar_2_4LEDs),
            PISUGAR_2_2LEDS => Ok(Model::PiSugar_2_2LEDs),
            PISUGAR_2_PRO => Ok(Model::PiSugar_2_Pro),
            PISUGAR_3 => Ok(Model::PiSugar_3),
            _ => Err(()),
        }
    }
}
