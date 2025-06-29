use std::collections::VecDeque;
use std::time::Instant;

use rppal::i2c::I2c;

use crate::Error;
use crate::{
    battery::{Battery, BatteryEvent},
    config::BatteryThreshold,
};
use crate::{convert_battery_voltage_to_level, I2cError, Model, PiSugarConfig};
use crate::{gpio_detect_tap, Result};

/// Battery threshold curve
pub const BATTERY_CURVE: [BatteryThreshold; 10] = [
    (4.10, 100.0),
    (4.05, 95.0),
    (3.90, 88.0),
    (3.80, 77.0),
    (3.70, 65.0),
    (3.62, 55.0),
    (3.58, 49.0),
    (3.49, 25.6),
    (3.32, 4.5),
    (3.1, 0.0),
];

/// Idle intensity
const PI_PRO_IDLE_INTENSITY: f64 = 0.2;

/// IP5312, pi-3/4 bat chip
pub struct IP5312 {
    i2c: I2c,
}

impl IP5312 {
    /// Create new IP5312
    pub fn new(i2c_bus: u8, i2c_addr: u16) -> Result<Self> {
        let mut i2c = I2c::with_bus(i2c_bus)?;
        i2c.set_slave_address(i2c_addr)?;
        Ok(Self { i2c })
    }

    /// Read voltage (V)
    pub fn read_voltage(&self) -> Result<f64> {
        let low = self.i2c.smbus_read_byte(0xd0)? as u16;
        let high = self.i2c.smbus_read_byte(0xd1)? as u16;

        if low == 0 && high == 0 {
            return Err(Error::I2c(I2cError::FeatureNotSupported));
        }

        let v = ((high & 0b0011_1111) << 8) + low;
        let v = (v as f64) * 0.26855 + 2600.0;
        Ok(v / 1000.0)
    }

    /// Parse level(%)
    pub fn parse_voltage_level(voltage: f32, curve: &[BatteryThreshold]) -> f32 {
        if voltage > 0.0 {
            convert_battery_voltage_to_level(voltage, curve)
        } else {
            100.0
        }
    }

    /// Read intensity (A)
    pub fn read_intensity(&self) -> Result<f64> {
        let low = self.i2c.smbus_read_byte(0xd2)? as u16;
        let high = self.i2c.smbus_read_byte(0xd3)? as u16;

        let intensity = if high & 0x20 != 0 {
            let i = (((high | 0b1100_0000) << 8) + low) as i16;
            (i as f64) * 2.68554
        } else {
            let i = ((high & 0x1f) << 8) + low;
            (i as f64) * 2.68554
        };
        Ok(intensity / 1000.0)
    }

    /// Shutdown under light load (126mA and 8s)
    pub fn enable_light_load_auto_shutdown(&self) -> Result<()> {
        // threshold intensity, x*4.3mA
        let x = PI_PRO_IDLE_INTENSITY * 1000_f64 / 4.3;
        let x = if x > 0b0011_1111 as f64 { 0b0011_1111 } else { x as u8 };
        let mut v = self.i2c.smbus_read_byte(0xc9)?;
        v &= 0b1100_0000;
        v |= x; // 47 * 4.3 = 200 ma
        self.i2c.smbus_write_byte(0xc9, v)?;

        // time, 8s
        let mut v = self.i2c.smbus_read_byte(0x06)?;
        v &= 0b0011_1111;
        self.i2c.smbus_write_byte(0x06, v)?;

        // enable
        let mut v = self.i2c.smbus_read_byte(0x03)?;
        v |= 0b0010_0000;
        self.i2c.smbus_write_byte(0x03, v)?;

        // enable bat low, 2.76-2.84V
        let mut v = self.i2c.smbus_read_byte(0x13)?;
        v &= 0b1100_1111;
        v |= 0b0001_0000;
        self.i2c.smbus_write_byte(0x13, v)?;

        Ok(())
    }

    /// Disable auto shutdown under light load
    pub fn disable_light_load_shutdown(&self) -> Result<()> {
        let mut v = self.i2c.smbus_read_byte(0x03)?;
        v &= 0b1101_1111;
        self.i2c.smbus_write_byte(0x03, v)?;
        Ok(())
    }

    /// Init GPIO, 4-led
    pub fn init_gpio(&self) -> Result<()> {
        // mfp_ctl0, set l4_sel
        let mut v = self.i2c.smbus_read_byte(0x52)?;
        v |= 0b0000_0010;
        self.i2c.smbus_write_byte(0x52, v)?;

        // gpio1 input
        let mut v = self.i2c.smbus_read_byte(0x54)?;
        v |= 0b0000_0010;
        self.i2c.smbus_write_byte(0x54, v)?;

        Ok(())
    }

    /// Init GPIO, 2-led
    pub fn init_gpio_2led(&self) -> Result<()> {
        // gpio1, l4 sel
        let mut v = self.i2c.smbus_read_byte(0x52)?;
        v |= 0b0000_0010;
        self.i2c.smbus_write_byte(0x52, v)?;

        // gpio1 input enable
        let mut v = self.i2c.smbus_read_byte(0x54)?;
        v |= 0b0000_0010;
        self.i2c.smbus_write_byte(0x54, v)?;

        // charging control, gpio2, light sel
        let mut v = self.i2c.smbus_read_byte(0x52)?;
        v |= 0b0000_0100;
        self.i2c.smbus_write_byte(0x52, v)?;

        // vset -> register
        let mut v = self.i2c.smbus_read_byte(0x29)?;
        v &= 0b1011_1111;
        self.i2c.smbus_write_byte(0x29, v)?;

        // vset fn adc
        let mut v = self.i2c.smbus_read_byte(0x52)?;
        v &= 0b1001_1111;
        v |= 0b0100_0000;
        self.i2c.smbus_write_byte(0x52, v)?;

        // vgpi enable
        let mut v = self.i2c.smbus_read_byte(0xc2)?;
        v |= 0b0001_0000;
        self.i2c.smbus_write_byte(0xc2, v)?;

        Ok(())
    }

    /// Allow/Disallow charging (0/1)
    pub fn allow_charging_2led(&self) -> Result<bool> {
        let v = self.i2c.smbus_read_byte(0x58)?;
        let allowed = (v & 0b0000_0100) == 0;
        Ok(allowed)
    }

    /// Enable/disable charging, 2 led only
    pub fn toggle_allow_charging_2led(&self, enable: bool) -> Result<()> {
        // gpio2 disable
        let mut v = self.i2c.smbus_read_byte(0x56)?;
        v &= 0b1111_1011;
        self.i2c.smbus_write_byte(0x56, v)?;

        // enable/disable
        let mut v = self.i2c.smbus_read_byte(0x58)?;
        v &= 0b1111_1011;
        if !enable {
            v |= 0b0000_0100;
        }
        self.i2c.smbus_write_byte(0x58, v)?;

        // gpio2 enable
        let mut v = self.i2c.smbus_read_byte(0x56)?;
        v |= 0b0000_0100;
        self.i2c.smbus_write_byte(0x56, v)?;

        Ok(())
    }

    /// Is power cable plugged in, 2-led
    pub fn is_power_plugged_2led(&self) -> Result<bool> {
        let high = self.i2c.smbus_read_byte(0xdd)?;
        if high == 0x1f {
            return Ok(true);
        }
        Ok(false)
    }

    /// Init boost intensity, 0x3f*50ma, 3A
    pub fn init_boost_intensity(&self) -> Result<()> {
        let mut v = self.i2c.smbus_read_byte(0x30)?;
        v &= 0b1100_0000;
        v |= 0x3f;
        self.i2c.smbus_write_byte(0x30, v)?;

        Ok(())
    }

    /// Read gpio tap, gpio1
    pub fn read_gpio_tap(&self) -> Result<u8> {
        let mut v = self.i2c.smbus_read_byte(0x58)?;
        v &= 0b0000_0010;

        Ok(v)
    }

    /// Force shutdown
    pub fn force_shutdown(&self) -> Result<()> {
        // enable auto shutdown
        self.enable_light_load_auto_shutdown()?;

        // enable force shutdown
        let mut t = self.i2c.smbus_read_byte(0x01)?;
        t &= 0b1111_1011;
        self.i2c.smbus_write_byte(0x01, t)?;

        Ok(())
    }
}

pub struct IP5312Battery {
    ip5312: IP5312,
    model: Model,
    voltages: VecDeque<(Instant, f32)>,
    intensities: VecDeque<(Instant, f32)>,
    levels: VecDeque<f32>,
    tap_history: String,
    cfg: PiSugarConfig,
}

impl IP5312Battery {
    pub fn new(cfg: PiSugarConfig, model: Model) -> Result<Self> {
        let ip5312 = IP5312::new(cfg.i2c_bus, cfg.i2c_addr.unwrap_or(model.default_battery_i2c_addr()))?;
        Ok(Self {
            ip5312,
            model,
            voltages: VecDeque::with_capacity(30),
            intensities: VecDeque::with_capacity(30),
            levels: VecDeque::with_capacity(30),
            tap_history: String::with_capacity(30),
            cfg,
        })
    }
}

impl Battery for IP5312Battery {
    fn init(&mut self, config: &PiSugarConfig) -> Result<()> {
        if self.model.led_amount() == 2 {
            self.ip5312.init_gpio_2led()?;
            self.ip5312.toggle_allow_charging_2led(true)?;
        } else {
            self.ip5312.init_gpio()?;
        }
        self.ip5312.init_boost_intensity()?;
        // NOTE: Disable auto shutdown in auto_power_on
        if config.auto_power_on == Some(true) {
            self.ip5312.disable_light_load_shutdown()?;
        } else {
            self.ip5312.enable_light_load_auto_shutdown()?;
        }

        let v = self.voltage()?;
        let now = Instant::now();
        while self.voltages.len() < self.voltages.capacity() {
            self.voltages.push_back((now, v));
        }

        let i = self.intensity()?;
        while self.intensities.len() > self.intensities.capacity() {
            self.intensities.push_back((now, i));
        }

        Ok(())
    }

    fn model(&self) -> String {
        self.model.to_string()
    }

    fn led_amount(&self) -> Result<u32> {
        Ok(self.model.led_amount())
    }

    fn version(&self) -> Result<String> {
        Ok("".to_string())
    }

    fn keep_input(&self) -> Result<bool> {
        Ok(true)
    }

    fn set_keep_input(&self, _enable: bool) -> Result<()> {
        Ok(())
    }

    fn voltage(&self) -> Result<f32> {
        self.ip5312.read_voltage().map(|v| v as f32)
    }

    fn voltage_avg(&self) -> Result<f32> {
        let mut total = 0.0;
        self.voltages.iter().for_each(|v| total += v.1);
        if !self.voltages.is_empty() {
            Ok(total / self.voltages.len() as f32)
        } else {
            Err(Error::Other("Require initialization".to_string()))
        }
    }

    fn level(&self) -> Result<f32> {
        let curve = self
            .cfg
            .battery_curve
            .as_ref()
            .map(|x| &x[..])
            .unwrap_or(BATTERY_CURVE.as_ref());
        self.voltage_avg().map(|x| IP5312::parse_voltage_level(x, curve))
    }

    fn intensity(&self) -> Result<f32> {
        self.ip5312.read_intensity().map(|i| i as f32)
    }

    fn intensity_avg(&self) -> Result<f32> {
        let mut total = 0.0;
        self.intensities.iter().for_each(|i| total += i.1);
        if !self.intensities.is_empty() {
            Ok(total / self.intensities.len() as f32)
        } else {
            Err(Error::Other("Require initialization".to_string()))
        }
    }

    fn is_power_plugged(&self) -> Result<bool> {
        if self.model.led_amount() == 2 {
            self.ip5312.is_power_plugged_2led()
        } else {
            self.is_charging()
        }
    }

    fn toggle_power_restore(&self, _enable: bool) -> Result<()> {
        Err(Error::Other("Not supported".to_string()))
    }

    fn is_allow_charging(&self) -> Result<bool> {
        if self.model.led_amount() == 2 {
            self.ip5312.allow_charging_2led()
        } else {
            Ok(true)
        }
    }

    fn toggle_allow_charging(&self, enable: bool) -> Result<()> {
        if self.model.led_amount() == 2 {
            self.ip5312.toggle_allow_charging_2led(enable)
        } else {
            Err(I2cError::FeatureNotSupported.into())
        }
    }

    fn is_charging(&self) -> Result<bool> {
        if self.levels.len() > 2 {
            if let Ok(avg) = self.voltage_avg() {
                return Ok(self.voltages[0].1 < avg && avg < self.voltages[self.voltages.len() - 1].1);
            }
        }
        Ok(false)
    }

    fn is_input_protected(&self) -> Result<bool> {
        Err(Error::Other("Not available".to_string()))
    }

    fn toggle_input_protected(&self, _enable: bool) -> Result<()> {
        Err(Error::Other("Not available".to_string()))
    }

    fn output_enabled(&self) -> Result<bool> {
        Ok(true)
    }

    fn toggle_output_enabled(&self, enable: bool) -> Result<()> {
        if !enable {
            return self.ip5312.force_shutdown();
        }
        Err(Error::Other("Not available".to_string()))
    }

    fn poll(&mut self, now: Instant, _config: &PiSugarConfig) -> Result<Vec<BatteryEvent>> {
        let voltage = self.voltage()?;
        self.voltages.pop_front();
        while self.voltages.len() < self.voltages.capacity() {
            self.voltages.push_back((now, voltage));
        }

        let level = self.level()?;
        self.levels.pop_front();
        while self.levels.len() < self.levels.capacity() {
            self.levels.push_back(level);
        }

        let intensity = self.intensity()?;
        self.intensities.pop_front();
        while self.intensities.len() < self.intensities.capacity() {
            self.intensities.push_back((now, intensity));
        }

        let gpio_value = self.ip5312.read_gpio_tap()?;
        let tapped = gpio_value != 0;
        if self.tap_history.len() >= self.tap_history.capacity() {
            self.tap_history.remove(0);
        }
        if tapped {
            self.tap_history.push('1');
        } else {
            self.tap_history.push('0');
        }

        let tap_result = gpio_detect_tap(&mut self.tap_history);

        let mut events = Vec::new();
        if let Some(tap_event) = tap_result {
            events.push(BatteryEvent::TapEvent(tap_event));
        }
        Ok(events)
    }

    fn toggle_light_load_shutdown(&self, enable: bool) -> Result<()> {
        if enable {
            self.ip5312.enable_light_load_auto_shutdown()
        } else {
            self.ip5312.disable_light_load_shutdown()
        }
    }

    fn toggle_soft_poweroff(&self, _enable: bool) -> Result<()> {
        Ok(())
    }

    fn toggle_anti_mistouch(&self, _: bool) -> Result<()> {
        Ok(())
    }

    fn temperature(&self) -> Result<f32> {
        Ok(0.0)
    }
}
