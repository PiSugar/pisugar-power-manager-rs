use std::collections::VecDeque;
use std::time::Instant;

use rppal::i2c::I2c;

use crate::battery::Battery;
use crate::{convert_battery_voltage_to_level, I2cError, MODEL_V2_PRO};
use crate::{gpio_detect_tap, Result, TapType};
use crate::{BatteryThreshold, Error};

/// Battery threshold curve
pub const BATTERY_CURVE: [BatteryThreshold; 10] = [
    (4.16, 100.0),
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
const PI_PRO_IDLE_INTENSITY: f64 = 0.25;

/// IP5312, pi-3/4 bat chip
pub struct IP5312 {
    i2c: I2c,
}

impl IP5312 {
    /// Create new IP5312
    pub fn new(i2c_addr: u16) -> Result<Self> {
        let mut i2c = I2c::new()?;
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
    pub fn parse_voltage_level(voltage: f64) -> f64 {
        let level = if voltage > 0.0 {
            convert_battery_voltage_to_level(voltage, &BATTERY_CURVE)
        } else {
            100.0
        };
        level
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
    pub fn init_auto_shutdown(&self) -> Result<()> {
        let threshold = PI_PRO_IDLE_INTENSITY * 1000.0;
        let threshold = (threshold / 4.3) as u64;
        let threshold = if threshold > 0b0011_1111 {
            0b0011_1111 as u8
        } else {
            threshold as u8
        };

        // threshold intensity, x*4.3mA = 126mA
        let mut v = self.i2c.smbus_read_byte(0xc9)?;
        v &= 0b1100_0000;
        v |= threshold;
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

    /// Enable/disable charging, 2 led only
    pub fn toggle_charging_2led(&self, enable: bool) -> Result<()> {
        // gpio2 disable
        let mut v = self.i2c.smbus_read_byte(0x56)?;
        v &= 0b1111_1011;
        self.i2c.smbus_write_byte(0x56, v)?;

        // enable/disable
        let mut v = self.i2c.smbus_read_byte(0x58)?;
        v &= 0b1111_1011;
        if enable {
            v |= 0b0000_0100;
        }
        self.i2c.smbus_write_byte(0x58, v)?;

        // gpio2 enable
        let mut v = self.i2c.smbus_read_byte(0x56)?;
        v |= 0b0000_0100;
        self.i2c.smbus_write_byte(0x56, v)?;

        Ok(())
    }

    /// Is charging, 2-led
    pub fn is_charging_2led(&self) -> Result<bool> {
        let low = self.i2c.smbus_read_byte(0xdc)?;
        let high = self.i2c.smbus_read_byte(0xdd)?;
        if low == 0xff && high == 0x1f {
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
        // enable force shutdown
        let mut t = self.i2c.smbus_read_byte(0x01)?;
        t &= 0b1111_1011;
        self.i2c.smbus_write_byte(0x01, t)?;

        Ok(())
    }
}

pub struct IP5312Battery {
    ip5312: IP5312,
    led_amount: u32,
    voltages: VecDeque<(Instant, f32)>,
    intensities: VecDeque<(Instant, f32)>,
    tap_history: String,
}

impl IP5312Battery {
    pub fn new(i2c_addr: u16, led_amount: u32) -> Result<Self> {
        let ip5312 = IP5312::new(i2c_addr)?;
        let voltages = VecDeque::with_capacity(10);
        let intensities = VecDeque::with_capacity(10);
        let tap_history = String::with_capacity(100);
        Ok(Self {
            ip5312,
            led_amount,
            voltages,
            intensities,
            tap_history,
        })
    }
}

impl Battery for IP5312Battery {
    fn init(&mut self) -> Result<()> {
        if self.led_amount == 2 {
            self.ip5312.init_gpio_2led()?;
            self.ip5312.toggle_charging_2led(true)?;
        } else {
            self.ip5312.init_gpio()?;
        }
        self.ip5312.init_boost_intensity()?;

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
        return MODEL_V2_PRO.to_string();
    }

    fn led_amount(&self) -> Result<u32> {
        Ok(self.led_amount)
    }

    fn voltage(&self) -> Result<f32> {
        self.ip5312.read_voltage().and_then(|v| Ok(v as f32))
    }

    fn voltage_avg(&self) -> Result<f32> {
        let mut total = 0.0;
        self.voltages.iter().for_each(|v| total += v.1);
        if self.voltages.len() > 0 {
            Ok(total / self.voltages.len() as f32)
        } else {
            Err(Error::Other("Require initialization".to_string()))
        }
    }

    fn level(&self) -> Result<f32> {
        self.voltage()
            .and_then(|v| Ok(IP5312::parse_voltage_level(v as f64) as f32))
    }

    fn intensity(&self) -> Result<f32> {
        self.ip5312.read_intensity().and_then(|i| Ok(i as f32))
    }

    fn intensity_avg(&self) -> Result<f32> {
        let mut total = 0.0;
        self.intensities.iter().for_each(|i| total += i.1);
        if self.intensities.len() > 0 {
            Ok(total / self.intensities.len() as f32)
        } else {
            Err(Error::Other("Require initialization".to_string()))
        }
    }

    fn is_charging(&self) -> Result<bool> {
        if self.led_amount == 2 {
            self.ip5312.is_charging_2led()
        } else {
            if self.voltages.len() >= 2 {
                let mut total = 0.0;
                for i in 1..self.voltages.len() {
                    let delta = self.voltages[i].1 - self.voltages[i - 1].1;
                    total += delta;
                }
                return Ok(total > 0.0);
            }
            Ok(false)
        }
    }

    fn toggle_charging(&self, enable: bool) -> Result<()> {
        if self.led_amount == 2 {
            self.ip5312.toggle_charging_2led(enable)
        } else {
            Err(I2cError::FeatureNotSupported.into())
        }
    }

    fn poll(&mut self, now: Instant) -> Result<Option<TapType>> {
        let voltage = self.voltage()?;
        if self.voltages.len() == self.voltages.capacity() {
            self.voltages.pop_front();
        }
        self.voltages.push_back((now, voltage));

        let intensity = self.intensity()?;
        if self.intensities.len() == self.intensities.capacity() {
            self.intensities.pop_front();
        }
        self.intensities.push_back((now, intensity));

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
        Ok(tap_result)
    }

    fn shutdown(&self) -> Result<()> {
        self.ip5312.force_shutdown()
    }
}
