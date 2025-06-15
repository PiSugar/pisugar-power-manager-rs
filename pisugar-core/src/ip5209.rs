use std::collections::VecDeque;
use std::time::Instant;

use rppal::i2c::I2c;

use crate::config::BatteryThreshold;
use crate::{
    battery::{Battery, BatteryEvent},
    I2C_ADDR_BAT,
};
use crate::{convert_battery_voltage_to_level, gpio_detect_tap, Error, Model, PiSugarConfig, Result};

/// Battery threshold curve
pub const BATTERY_CURVE: [BatteryThreshold; 10] = [
    (4.16, 100.0),
    (4.05, 95.0),
    (4.00, 80.0),
    (3.92, 65.0),
    (3.86, 40.0),
    (3.79, 25.5),
    (3.66, 10.0),
    (3.52, 6.5),
    (3.49, 3.2),
    (3.1, 0.0),
];

/// Idle intensity
const PI_ZERO_IDLE_INTENSITY: f64 = 0.11;

/// IP5209, pi-zero bat chip
pub struct IP5209 {
    i2c: I2c,
}

impl IP5209 {
    /// Create new IP5209
    pub fn new(i2c_bus: u8, i2c_addr: u16) -> Result<Self> {
        let mut i2c = I2c::with_bus(i2c_bus)?;
        i2c.set_slave_address(i2c_addr)?;
        Ok(Self { i2c })
    }

    /// Read voltage (V)
    pub fn read_voltage(&self) -> Result<f64> {
        let low = self.i2c.smbus_read_byte(0xa2)? as u16;
        let high = self.i2c.smbus_read_byte(0xa3)? as u16;

        // check negative values
        let voltage = if high & 0x20 == 0x20 {
            let v = (((high | 0b1100_0000) << 8) + low) as i16;
            2600.0 - (v as f64) * 0.26855
        } else {
            let v = ((high & 0x1f) << 8) + low;
            2600.0 + (v as f64) * 0.26855
        };

        Ok(voltage / 1000.0)
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
        let low = self.i2c.smbus_read_byte(0xa4)? as u16;
        let high = self.i2c.smbus_read_byte(0xa5)? as u16;

        // check negative value
        let intensity = if high & 0x20 == 0x20 {
            let i = (((high | 0b1100_0000) << 8) + low) as i16;
            (i as f64) * 0.745985
        } else {
            let i = ((high & 0x1f) << 8) + low;
            (i as f64) * 0.745985
        };

        Ok(intensity / 1000.0)
    }

    /// Shutdown under light load (144mA and 8s)
    pub fn enable_light_load_auto_shutdown(&self) -> Result<()> {
        let threshold = PI_ZERO_IDLE_INTENSITY * 1000.0;
        let threshold = (threshold / 12.0) as u64;
        let threshold = if threshold > 0b0001_1111 {
            0b0001_1111_u8
        } else {
            threshold as u8
        };

        // threshold intensity, x*12mA = 108mA
        let mut v = self.i2c.smbus_read_byte(0x0c)?;
        v &= 0b0000_0111;
        v |= threshold << 3;
        self.i2c.smbus_write_byte(0x0c, v)?;

        // time, 8s
        let mut v = self.i2c.smbus_read_byte(0x04)?;
        v &= 0b00111111;
        self.i2c.smbus_write_byte(0x04, v)?;

        // enable auto shutdown and turn on
        let mut v = self.i2c.smbus_read_byte(0x02)?;
        v |= 0b0000_0011;
        self.i2c.smbus_write_byte(0x02, v)?;

        Ok(())
    }

    /// Disable auto shutdown under light load
    pub fn disable_light_load_shutdown(&self) -> Result<()> {
        let mut v = self.i2c.smbus_read_byte(0x02)?;
        v &= 0b1111_1101;
        self.i2c.smbus_write_byte(0x02, v)?;
        Ok(())
    }

    /// Enable GPIO, 4-led
    pub fn init_gpio(&self) -> Result<()> {
        // vset
        let mut v = self.i2c.smbus_read_byte(0x26)?;
        v |= 0b0000_0000;
        v &= 0b1011_1111;
        self.i2c.smbus_write_byte(0x26, v)?;

        // vset -> gpio
        let mut v = self.i2c.smbus_read_byte(0x52)?;
        v |= 0b0000_0100;
        v &= 0b1111_0111;
        self.i2c.smbus_write_byte(0x52, v)?;

        // enable gpio input
        let mut v = self.i2c.smbus_read_byte(0x53)?;
        v |= 0b0001_0000;
        v &= 0b1111_1111;
        self.i2c.smbus_write_byte(0x53, v)?;

        Ok(())
    }

    /// Init GPIO, 2-led
    pub fn init_gpio_2led(&self) -> Result<()> {
        // gpio1 tap, L4 sel
        let mut v = self.i2c.smbus_read_byte(0x51)?;
        v &= 0b1111_0011;
        v |= 0b0000_0100;
        self.i2c.smbus_write_byte(0x51, v)?;

        // gpio1 input enable
        let mut v = self.i2c.smbus_read_byte(0x53)?;
        v |= 0b0000_0010;
        self.i2c.smbus_write_byte(0x53, v)?;

        // charging control, gpio2
        let mut v = self.i2c.smbus_read_byte(0x51)?;
        v &= 0b1100_1111;
        v |= 0b0001_0000;
        self.i2c.smbus_write_byte(0x51, v)?;

        // vset -> register
        let mut v = self.i2c.smbus_read_byte(0x26)?;
        v &= 0b1011_0000;
        self.i2c.smbus_write_byte(0x26, v)?;

        // vset -> gpio4
        let mut v = self.i2c.smbus_read_byte(0x52)?;
        v &= 0b1111_0011;
        v |= 0b0000_0100;
        self.i2c.smbus_write_byte(0x52, v)?;

        // gpio4 input enable
        let mut v = self.i2c.smbus_read_byte(0x53)?;
        v &= 0b1110_1111;
        v |= 0b0001_0000;
        self.i2c.smbus_write_byte(0x53, v)?;

        Ok(())
    }

    /// Allow/Disallow charging (0/1)
    pub fn allow_charging_2led(&self) -> Result<bool> {
        let v = self.i2c.smbus_read_byte(0x55)?;
        let allowed = (v & 0b0000_0100) == 0;
        Ok(allowed)
    }

    /// Enable/Disable charging, 2 led version
    pub fn toggle_allow_charging_2led(&self, enable: bool) -> Result<()> {
        // disable gpio2 output
        let mut v = self.i2c.smbus_read_byte(0x54)?;
        v &= 0b1111_1011;
        self.i2c.smbus_write_byte(0x54, v)?;

        // enable or disable charging
        let mut v = self.i2c.smbus_read_byte(0x55)?;
        v &= 0b1111_1011;
        if !enable {
            v |= 0b0000_0100;
        }
        self.i2c.smbus_write_byte(0x55, v)?;

        // enable gpio2 output
        let mut v = self.i2c.smbus_read_byte(0x54)?;
        v &= 0b1111_1011;
        v |= 0b0000_0100;
        self.i2c.smbus_write_byte(0x54, v)?;

        Ok(())
    }

    /// Is power cable plugged in
    pub fn is_power_plugged_2led(&self) -> Result<bool> {
        let v = self.i2c.smbus_read_byte(0x55)?;
        if v & 0b0001_0000 != 0 {
            return Ok(true);
        }
        Ok(false)
    }

    /// Read gpio tap 4:0, gpio4 / gpio1
    pub fn read_gpio_tap(&self) -> Result<u8> {
        let v = self.i2c.smbus_read_byte(0x55)?;
        Ok(v)
    }

    /// Force shutdown
    pub fn force_shutdown(&self) -> Result<()> {
        // enable auto shutdown
        self.enable_light_load_auto_shutdown()?;

        // force shutdown
        let mut t = self.i2c.smbus_read_byte(0x01)?;
        t &= 0b1111_1011;
        self.i2c.smbus_write_byte(0x01, t)?;

        Ok(())
    }
}

pub struct IP5209Battery {
    ip5209: IP5209,
    model: Model,
    voltages: VecDeque<(Instant, f32)>,
    levels: VecDeque<f32>,
    intensities: VecDeque<(Instant, f32)>,
    tap_history: String,
    cfg: PiSugarConfig,
}

impl IP5209Battery {
    pub fn new(cfg: PiSugarConfig, model: Model) -> Result<Self> {
        let ip5209 = IP5209::new(cfg.i2c_bus, cfg.i2c_addr.unwrap_or(I2C_ADDR_BAT))?;
        Ok(Self {
            ip5209,
            model,
            voltages: VecDeque::with_capacity(30),
            intensities: VecDeque::with_capacity(30),
            levels: VecDeque::with_capacity(30),
            tap_history: String::with_capacity(30),
            cfg,
        })
    }
}

impl Battery for IP5209Battery {
    fn init(&mut self, config: &PiSugarConfig) -> Result<()> {
        if self.model.led_amount() == 2 {
            self.ip5209.init_gpio_2led()?;
            self.ip5209.toggle_allow_charging_2led(true)?;
        } else {
            self.ip5209.init_gpio()?;
        }
        // NOTE: Disable auto shutdown in auto_power_on
        if config.auto_power_on == Some(true) {
            self.ip5209.disable_light_load_shutdown()?;
        } else {
            self.ip5209.enable_light_load_auto_shutdown()?;
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
        self.ip5209.read_voltage().map(|v| v as f32)
    }

    fn voltage_avg(&self) -> Result<f32> {
        let mut total = 0.0;
        self.voltages.iter().for_each(|v| total += v.1);
        if !self.voltages.is_empty() {
            Ok(total / self.voltages.len() as f32)
        } else {
            Err(Error::Other("Required initialization".to_string()))
        }
    }

    fn level(&self) -> Result<f32> {
        let curve = self
            .cfg
            .battery_curve
            .as_ref()
            .map(|x| &x[..])
            .unwrap_or(BATTERY_CURVE.as_ref());
        self.voltage_avg().map(|x| IP5209::parse_voltage_level(x, curve))
    }

    fn intensity(&self) -> Result<f32> {
        self.ip5209.read_intensity().map(|x| x as f32)
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
            self.ip5209.is_power_plugged_2led()
        } else {
            self.is_charging()
        }
    }

    fn toggle_power_restore(&self, _enable: bool) -> Result<()> {
        Err(Error::Other("Not supported".to_string()))
    }

    fn is_allow_charging(&self) -> Result<bool> {
        if self.model.led_amount() == 2 {
            self.ip5209.allow_charging_2led()
        } else {
            Ok(true)
        }
    }

    fn toggle_allow_charging(&self, enable: bool) -> Result<()> {
        if self.model.led_amount() == 2 {
            self.ip5209.toggle_allow_charging_2led(enable)
        } else {
            Err(Error::Other("Not supported".to_string()))
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
            return self.ip5209.force_shutdown();
        }
        Err(Error::Other("Not available".to_string()))
    }

    fn poll(&mut self, now: Instant, _config: &PiSugarConfig) -> Result<Vec<BatteryEvent>> {
        let voltage = self.voltage()?;
        if self.voltages.len() >= self.voltages.capacity() {
            self.voltages.pop_front();
        }
        self.voltages.push_back((now, voltage));

        let level = self.level()?;
        if self.levels.len() >= self.levels.capacity() {
            self.levels.pop_front();
        }
        self.levels.push_back(level);

        let intensity = self.intensity()?;
        if self.intensities.len() >= self.intensities.capacity() {
            self.intensities.pop_front();
        }
        self.intensities.push_back((now, intensity));

        let gpio_value = self.ip5209.read_gpio_tap()?;
        let tapped = if self.model.led_amount() == 2 {
            gpio_value & 0b0000_0010 != 0 // GPIO1 in 2-led
        } else {
            gpio_value & 0b0001_0000 != 0 // GPIO4 in 4-led
        };

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
            self.ip5209.enable_light_load_auto_shutdown()
        } else {
            self.ip5209.disable_light_load_shutdown()
        }
    }

    fn toggle_soft_poweroff(&self, _enable: bool) -> Result<()> {
        Ok(())
    }

    fn toggle_anti_mistouch(&self, _: bool) -> std::result::Result<(), Error> {
        Ok(())
    }

    fn temperature(&self) -> std::result::Result<f32, Error> {
        Ok(0.0)
    }
}
