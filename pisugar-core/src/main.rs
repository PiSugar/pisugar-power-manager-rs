#[macro_use]
extern crate num_derive;

use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::thread;
use std::thread::Thread;
use std::time::{Duration, Instant};

use num_traits::{FromPrimitive, ToPrimitive};
use rppal::i2c::Error as I2cError;
use rppal::i2c::I2c;

const TIME_HOST: &str = "cdn.pisugar.com";

/// RTC address
pub const I2C_ADDR_RTC: u16 = 0x32;
pub const I2C_RTC_CTR1: u8 = 0x0f;
pub const I2C_RTC_CTR2: u8 = 0x10;
pub const I2C_RTC_CTR3: u8 = 0x11;

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
#[derive(FromPrimitive, ToPrimitive)]
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

/// Battery voltage to percentage level
pub fn convert_battery_voltage_to_level(voltage: f64) -> f64 {
    if voltage > 5.5 {
        return 100.0;
    }
    for threshold in &BATTERY_CURVE {
        if voltage >= threshold.0 {
            let mut percentage = (voltage - threshold.0) / (threshold.1 - threshold.0);
            let level = threshold.2 + percentage * (threshold.3 - threshold.2);
            return level;
        }
    }
    0.0
}

/// Read battery voltage
fn read_battery_voltage() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;

    let low = i2c.smbus_read_byte(I2C_CMD_READ_VOLTAGE_LOW)? as u16;
    let high = i2c.smbus_read_byte(I2C_CMD_READ_VOLTAGE_HIGH)? as u16;
    log::debug!("voltage low: 0x{:x}, high: 0x{:x}", low, high);

    // check negative values
    let voltage = if high & 0x20 == 0x20 {
        let v = (!(((high & 0x1f) << 8) + low) as i16) + 1;
        2600.0 - (v as f64) * 0.26855
    } else {
        let v = ((high & 0x1f) << 8) + low;
        2600.0 + v as f64 * 0.26855
    };

    Ok(voltage / 1000.0)
}

/// Read battery current intensity
fn read_battery_intensity() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;

    let low = i2c.smbus_read_byte(I2C_CMD_READ_INTENSITY_LOW)? as u16;
    let high = i2c.smbus_read_byte(I2C_CMD_READ_INTENSITY_HIGH)? as u16;
    log::debug!("intensity low: 0x{:x}, high: 0x{:x}", low, high);

    let intensity = if high & 0x20 == 0x20 {
        let v = (!(((high & 0x1f) << 8) + low) as i16) + 1;
        -(v as f64) * 0.745985
    } else {
        let v = ((high & 0x1f) << 8) + low;
        v as f64 * 0.745985
    };

    Ok(intensity / 1000.0)
}

/// Read battery pro intensity
fn read_battery_pro_intensity() -> Result<f64> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;
    let low = i2c.smbus_read_byte(I2C_CMD_READ_PRO_INTENSITY_LOW)?;
    let high = i2c.smbus_read_byte(I2C_CMD_READ_PRO_INTENSITY_HIGH)?;
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

/// Set shutdown threshold
fn set_battery_shutdown_threshold() -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_BAT)?;
    unimplemented!()
}

fn rtc_disable_write_protect() -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_RTC)?;

    let mut data = i2c.smbus_read_byte(I2C_RTC_CTR2)?;
    data |= 0b1000_0000;
    i2c.smbus_write_byte(I2C_RTC_CTR2, data);

    data = i2c.smbus_read_byte(I2C_RTC_CTR1)?;
    data |= 0b1000_0100;
    i2c.smbus_write_byte(I2C_RTC_CTR1, data)?;

    Ok(())
}

fn rtc_enable_write_protect() -> Result<()> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_RTC)?;

    let mut data = i2c.smbus_read_byte(I2C_RTC_CTR1)?;
    data &= 0b0111_1011;
    i2c.smbus_write_byte(I2C_RTC_CTR1, data);

    data = i2c.smbus_read_byte(I2C_RTC_CTR2)?;
    data &= 0b0111_1111;
    i2c.smbus_write_byte(I2C_RTC_CTR2, data)?;

    Ok(())
}

pub fn rtc_read_alarm_flag() -> Result<bool> {
    let mut i2c = I2c::new()?;
    i2c.set_slave_address(I2C_ADDR_RTC)?;

    let data = i2c.smbus_read_byte(I2C_RTC_CTR1)?;
    if data & 0b0010_0000 != 0 || data & 0b0001_0000 != 0 {
        return Ok(true);
    }

    Ok(false)
}

pub fn rtc_clean_alarm_flag() -> Result<()> {
    match rtc_read_alarm_flag() {
        Ok(true) => {
            rtc_disable_write_protect()?;
            let mut i2c = I2c::new()?;
            i2c.set_slave_address(I2C_ADDR_RTC)?;

            let mut data = i2c.smbus_read_byte(I2C_RTC_CTR1)?;
            data &= 0b1100_1111;
            i2c.smbus_write_byte(I2C_RTC_CTR1, data)?;

            rtc_enable_write_protect()?;
        }
        _ => {}
    }
    Ok(())
}

fn bcd_to_dec(bcd: u8) -> u8 {
    (bcd & 0x0F) + (((bcd & 0xF0) >> 4) * 10)
}

fn dec_to_bcd(dec: u8) -> u8 {
    dec % 10 + ((dec / 10) << 4)
}

pub const MODEL_V2: &str = "PiSugar 2";
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
    model: &'static str,
    voltage: f64,
    intensity: f64,
    level: f64,
    level_records: VecDeque<f64>,
    charging: bool,
    updated_at: Instant,
}

impl PiSugarBatteryStatus {
    pub fn new() -> Self {
        let mut level_records = VecDeque::with_capacity(10);

        let mut model = MODEL_V2_PRO;
        let voltage = match read_battery_voltage() {
            Ok(voltage) => {
                model = MODEL_V2;
                voltage
            }
            _ => { 0.0 }
        };
        let level = convert_battery_voltage_to_level(voltage);

        let intensity = match read_battery_intensity() {
            Ok(intensity) => {
                intensity
            }
            _ => { 0.0 }
        };

        for i in 0..level_records.capacity() {
            level_records.push_back(level);
        }

        Self {
            model,
            voltage,
            intensity,
            level,
            level_records,
            charging: false,
            updated_at: Instant::now(),
        }
    }

    /// PiSugar model
    pub fn mode(&self) -> &'static str {
        self.model
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
        if self.updated_at + 3 * I2C_READ_INTERVAL >= now {
            return true;
        }
        false
    }

    /// PiSugar is charging, with voltage linear regression
    pub fn is_charging(&self, now: Instant) -> bool {
        if self.is_alive(now) {
            log::debug!("levels: {:?}", self.level_records);
            let capacity = self.level_records.capacity() as f64;
            let mut x_sum = (0.0 + capacity - 1.0) * capacity / 2.0;
            let x_bar = x_sum / capacity;
            let mut y_sum: f64 = self.level_records.iter().sum();
            let y_bar = y_sum / capacity;
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
            return k >= 0.005;
        }
        false
    }
}

/// Infinity loop to read battery status
pub fn start_pisugar_loop(status: Arc<RwLock<PiSugarBatteryStatus>>) {
    let handler = thread::spawn(move || {
        log::info!("PiSugar batter loop started");
        loop {
            let now = Instant::now();
            if let Ok(mut status) = status.write() {
                if let Ok(v) = read_battery_voltage() {
                    log::debug!("voltage: {}", v);
                    status.update_voltage(v, now);
                }
                if let Ok(i) = read_battery_intensity() {
                    log::debug!("intensity: {}", i);
                    status.update_intensity(i, now);
                }
            }
            // sleep
            thread::sleep(I2C_READ_INTERVAL);
        }
    });
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

fn main() {
    env_logger::init();

    let status = Arc::new(RwLock::new(PiSugarBatteryStatus::new()));

    start_pisugar_loop(status.clone());

    for i in 0..10 {
        let now = Instant::now();
        if let Ok(status) = status.read() {
            log::info!("battery status => {}V, {}A, active: {}, charging: {}, level: {}",  status.voltage(), status.intensity(), status.is_alive(now), status.is_charging(now), status.level());
        }
        thread::sleep(Duration::from_secs(1));
    }
}
