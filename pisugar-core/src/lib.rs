#[macro_use]
extern crate num_derive;

use num_traits::{FromPrimitive, ToPrimitive};
use rppal::i2c::Error as I2cError;
use rppal::i2c::I2c;
use std::collections::VecDeque;
use std::time::{Instant, Duration};


const TIME_HOST: &str = "cdn.pisugar.com";

/// RTC address
pub const I2C_ADDR_RTC: u16 = 0x32;

/// Battery address
pub const I2C_ADDR_BAT: u16 = 0x75;

const I2C_CMD_READ_INTENSITY_LOW: u8 = 0xa4;
const I2C_CMD_READ_INTENSITY_HIGH: u8 = 0xa5;
const I2C_CMD_READ_VOLTAGE_LOW: u8 = 0xa2;
const I2C_CMD_READ_VOLTAGE_HIGH: u8 = 0xa3;
const I2C_CMD_READ_PRO_INTENSITY_LOW: u8 = 0x66;
const I2C_CMD_READ_PRO_INTENSITY_HIGH: u8 = 0x67;
const I2C_CMD_READ_PRO_VOLTAGE_LOW: u8 = 0x64;
const I2C_CMD_READ_PRO_VOLTAGE_HIGH: u8 = 0x65;

const I2C_READ_INTERVAL: Duration = Duration::from_secs(1);

/// IP5209/IP5109/IP5207/IP5108 register address
#[allow(non_camel_case_types)]
#[derive(FromPrimitive, ToPrimive)]
enum IP5209Register {
    /// SYS_CTL0
    /// ```txt
    /// 3 - enable/disable light, rw
    /// 2 - enable/disable boost, rw
    /// 1 - enable/disable charger, rw
    /// ```
    SYS_CTL0 = 0x01,

    /// SYS_CTL1
    /// ```txt
    /// 1 - automatically shutdown, rw
    /// 0 - automatically turn on, rw
    /// ```
    SYS_CTL1 = 0x02,

    /// SYS_CTL2
    /// ```txt
    /// 7:3 - shutdown intensity threshold last for least 32s, n*12mA (at least 100mA), rw
    /// ```
    SYS_CTL2 = 0x0c,

    /// SYS_CTL3
    /// ```txt
    /// 7:6 - long press time
    ///       00 1s
    ///       01 2s
    ///       10 3s
    ///       11 4s
    /// 5 - enable/disable double-click shutdown, rw
    /// ```
    SYS_CTL3 = 0x03,

    /// SYS_CTL4
    /// ```txt
    /// 7:6 - shutdown countdown, rw
    ///       00 8s
    ///       01 16s
    ///       10 32s
    ///       11 64s
    /// 5 - auto enable boost when VIN is unplugged
    /// ```
    SYS_CTL4 = 0x04,

    /// SYS_CTL5
    /// ```txt
    /// 6 - enable/disable NTC, rw
    /// 1 - WLED, not supported
    /// 0 - poweroff method
    ///     0 double-click
    ///     1 long press for 2s
    /// ```
    SYS_CTL5 = 0x07,

    /// Charger_CTL1
    /// ```txt
    /// 3:2 - output voltage when charging, rw
    ///       11 4.83V
    ///       10 4.73V
    ///       01 4.63V
    ///       00 4.53V
    /// ```
    Charger_CTL1 = 0x22,

    /// Charger_CTL2
    /// ```txt
    /// 6:5 - Baterry type, rw
    ///       11 reserved
    ///       10 4.35V
    ///       01 4.3V
    ///       00 4.2V
    /// 2:1 - Constant voltage
    ///       11 add 42mV
    ///       10 add 28mV
    ///       01 add 14mV
    ///       00 no
    /// ```
    Charget_CTL2 = 0x24,

    /// CHG_DIG_CTL0
    CHG_DIG_CTL0 = 0x26,

    /// CHG_DIG_CTL1
    CHG_DIG_CTL1 = 0x25,

    /// MFP_CTL0
    MFP_CTL0 = 0x51,

    /// MFP_CTL1
    MFP_CTL1 = 0x52,

    /// GPIO_CTL1
    GPIO_CTL1 = 0x53,

    /// GPIO_CTL2
    GPIO_CTL2 = 0x54,

    /// GPIO_CTL3
    GPIO_CTL3 = 0x55,

    /// BATVADC_DAT0
    /// ```txt
    /// 7:0 - BATVADC low 8 bits, r
    /// ```
    BATVADC_DAT0 = 0xa2,

    /// BATVADC_DAT1
    /// ```txt
    /// 5:0 - BATVADC high 6 bits, r
    /// ```
    BATVADC_DAT1 = 0xa3,

    /// BATIADC_DAT0
    /// ```txt
    /// 7:0 - BATIADC low 8 bits, r
    /// ```
    BATIADC_DAT0 = 0xa4,

    /// BATIADC_DAT1
    /// ```txt
    /// 5:0 - BATIADC high 6 bits, r
    /// ```
    BATIADC_DAT1 = 0xa5,

    /// BATOCV_DAT0
    /// ```txt
    /// 7:0 - BATOAC low 8 bits, r
    /// ```
    BATOCV_DAT0 = 0xa8,

    /// BATOCV_DAT1
    /// ```txt
    /// 5:0 - BATOAC high 6 bits, r
    /// ```
    BATOCV_DAT1 = 0xa9,

    /// Reg_READ0,
    /// ```txt
    /// 3 - Charging status
    ///     0
    ///     1
    /// ```
    Reg_READ0 = 0x70,

    /// Reg_READ0_B (Reg_READ0 in document)
    /// ```txt
    /// 7:5 - Charging status
    ///       000 idle
    ///       001
    ///       010
    ///       011
    ///       110
    /// 3 - Charging end flag
    ///     0
    ///     1
    /// 2 - Constant charging timeout
    ///     0
    ///     1
    /// 1 - Charging timeout
    ///     0
    ///     1
    /// 0 - Charing timeout
    ///     0
    ///     1
    /// ```
    Reg_READ0_B = 0x71,

    /// Reg_READ1 (Reg_READ1 in document)
    /// ```txt
    /// 7 - Have light led
    ///     0 Yes
    ///     1 No
    /// 6 - Low load flag
    ///     0 Heavy load (>75mA)
    ///     1 Light load (<75mA)
    /// ```
    Reg_READ1 = 0x72,

    /// Reg_READ2 (Reg_READ2 in document)
    /// ```txt
    /// 3 - Button flag, r
    ///     0 Not pressed
    ///     1 Button down
    /// 1 - Long pressed flag, r
    ///     0
    ///     1 Pressed
    /// 0 - Short pressed flag, r
    ///     0
    ///     1
    /// ```
    Reg_READ2 = 0x77,
}

enum IP5312Register {}

/// SD3078 register address
enum SD3078Register {
    CTR1 = 0x0f,
    CTR2 = 0x10,
    CTR3 = 0x11,
}

/// PiSugar error
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

/// Battery voltage to percentage
pub fn battery_voltage_percentage(voltage: f64) -> f64 {
    if voltage > 5.5 {
        return 100.0;
    }
    for threshold in &BATTERY_CURVE {
        if voltage >= threshold.0 && voltage < threshold.1 {
            let mut percentage = (voltage - threshold.0) / (threshold.1 - threshold.0);
            percentage *= threshold.3 - threshold.2;
            percentage += threshold.2 + percentage;
            return percentage;
        }
    }
    0.0
}

/// Read battery current intensity
fn read_battery_intensity() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;
    let low = i2c.smbus_read_byte(IP5209Register::BATIADC_DAT0.to_u8())?;
    let high = i2c.smbus_read_byte(IP5209Register::BATIADC_DAT1.to_u8())?;
    let intensity = if high & 0x20 != 0 {
        let low = (!low) as u16;
        let high = (!high & 0x1f) as u16;
        -((high << 8 + low + 1) as f64) * 0.745985
    } else {
        let low = low as u16;
        let high = (high & 0x1f) as u16;
        (high << 8 + low + 1) as f64 * 0.745985
    };
    Ok(intensity)
}

/// Read battery voltage
fn read_battery_voltage() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;
    let low = i2c.smbus_read_byte(IP5209Register::BATVADC_DAT0.to_u8())?;
    let high = i2c.smbus_read_byte(IP5209Register::BATVADC_DAT1.to_u8())?;
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

/// Read battery pro intensity
fn read_battery_pro_intensity() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;
    let low = i2c.smbus_read_byte(IP5209Register::BATIADC_DAT0.to_u8())?;
    let high = i2c.smbus_read_byte(IP5209Register::BATIADC_DAT0.to_u8())?;
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
fn read_battery_pro_voltage() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;
    let low = i2c.smbus_read_byte(I2C_CMD_READ_PRO_VOLTAGE_LOW)?;
    let high = i2c.smbus_read_byte(I2C_CMD_READ_PRO_VOLTAGE_HIGH)?;
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

fn set_battery_shutdown_threshold() -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;
    unimplemented!()
}

pub const MODEL_V2: &str = "PiSugar 2 Pro";
pub const MODEL_V2_PRO: &str = "PiSugar 2 Pro";

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

pub struct PiSugarBatteryStatus {
    pub voltage: f64,
    pub intensity: f64,
    pub level: f64,
    pub level_records: VecDeque<f64>,
    pub charging: bool,
    pub updated_at: Instant,
}

impl PiSugarBatteryStatus {
    pub fn new() -> Self {
        Self {
            voltage: 0.0,
            intensity: 0.0,
            level: 0.0,
            level_records: VecDeque::with_capacity(128),
            charging: false,
            updated_at: Instant::now(),
        }
    }

    /// PiSugar battery alive
    pub fn is_alive(&self, now: Instant) -> bool {
        if self.updated_at + I2C_READ_INTERVAL >= now {
            return true;
        }
        false
    }

    /// PiSugar is charging
    pub fn is_charging(&self, now: Instant) -> bool {
        if self.is_alive(now) {
        }
        false
    }
}

pub struct PiSugarCore {
    config: PiSugarConfig,
}

impl PiSugarCore {
    pub fn new(config: PiSugarConfig) -> Self {
        Self { config }
    }

    /// Get PiSugar model
    pub fn get_model(&self) -> &str {
        if let Ok(voltage) = read_battery_voltage() {
            if voltage > 0.1 {
                return MODEL_V2;
            }
        }
        return MODEL_V2_PRO;
    }

    /// Get battery level
    pub fn get_battery_level(&self) -> &str {
        unimplemented!()
    }

    /// Get battery voltage
    pub fn get_battery_voltage(&self) -> Result<f64> {
        read_battery_voltage()
    }

    /// Get battery intensity
    pub fn get_battery_intensity(&self) -> Result<f64> {
        read_battery_intensity()
    }

    /// Is battery charging
    pub fn is_charging(&self) -> bool {
        unimplemented!()
    }

    /// Get RTC time
    pub fn get_rtc_time(&self) {
        unimplemented!()
    }

    /// Get RTC time list
    pub fn get_rtc_time_list(&self) {
        unimplemented!()
    }

    /// Get RTC alarm enable or disable
    pub fn get_rtc_alarm_flag(&self) -> bool {
        unimplemented!()
    }

    pub fn get_rtc_alarm_type(&self) {
        unimplemented!()
    }

    pub fn get_rtc_alarm_time(&self) {
        unimplemented!()
    }

    pub fn get_rtc_alarm_repeat(&self) -> bool {
        unimplemented!()
    }

    pub fn get_safe_shutdown_level(&self) {
        unimplemented!()
    }

    pub fn is_button_enabled(&self) -> bool {
        unimplemented!()
    }

    pub fn get_button_shell(&self) -> &str {
        unimplemented!()
    }

    pub fn rtc_clear_alarm_flag(&self) {
        unimplemented!()
    }

    pub fn sync_time_pi2rtc(&self) {
        unimplemented!()
    }

    pub fn sync_time_rtc2pi(&self) {
        unimplemented!()
    }

    pub fn sync_time_web2rtc(&self) {
        unimplemented!()
    }

    pub fn set_rtc_alarm(&self) {
        unimplemented!()
    }

    pub fn disable_rtc_alarm(&self) {
        unimplemented!()
    }

    pub fn set_safe_shutdown_level(&self) {
        unimplemented!()
    }

    pub fn set_test_wakeup(&self) {
        unimplemented!()
    }

    pub fn set_button_enable(&self) {
        unimplemented!()
    }

    pub fn set_button_shell(&self, shell_script: &str) {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_const() {
        let s = TIME_HOST;
        assert_eq!(s, "cdn.pisugar.com")
    }

    #[test]
    fn test_read_voltage() {
        let r = read_battery_voltage();
        assert!(r.is_ok())
    }
}
