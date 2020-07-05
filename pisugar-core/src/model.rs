use crate::battery::Battery;
use crate::ip5209::IP5209Battery;
use std::convert::TryFrom;
use std::fmt;

use crate::ip5312::IP5312Battery;
use crate::Result;

const PISUGAR_2_4LEDS: &str = "PiSugar 2 (4-LEDs)";
const PISUGAR_2_2LEDS: &str = "PiSugar 2 (2-LEDs)";
const PISUGAR_2_PRO: &str = "PiSugar 2 Pro";

/// PiSugar 2 model
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[allow(non_camel_case_types)]
pub enum Model {
    PiSugar_2_4LEDs,
    PiSugar_2_2LEDs,
    PiSugar_2_Pro,
}

impl Model {
    pub fn led_amount(&self) -> u32 {
        match *self {
            Model::PiSugar_2_4LEDs => 4,
            Model::PiSugar_2_2LEDs => 2,
            Model::PiSugar_2_Pro => 2,
        }
    }

    pub fn bind(&self, i2c_addr: u16) -> Result<Box<dyn Battery + Send>> {
        let b: Box<dyn Battery + Send> = match *self {
            Model::PiSugar_2_4LEDs => Box::new(IP5209Battery::new(i2c_addr, *self)?),
            Model::PiSugar_2_2LEDs => Box::new(IP5209Battery::new(i2c_addr, *self)?),
            Model::PiSugar_2_Pro => Box::new(IP5312Battery::new(i2c_addr, *self)?),
        };
        Ok(b)
    }
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Model::PiSugar_2_4LEDs => PISUGAR_2_4LEDS,
            Model::PiSugar_2_2LEDs => PISUGAR_2_2LEDS,
            Model::PiSugar_2_Pro => PISUGAR_2_PRO,
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
            _ => Err(()),
        }
    }
}
