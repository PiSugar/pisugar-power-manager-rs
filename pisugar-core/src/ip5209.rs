use std::collections::VecDeque;
use std::time::Instant;

use rppal::i2c::I2c;

use crate::battery::Battery;
use crate::{
    convert_battery_voltage_to_level, gpio_detect_tap, BatteryThreshold, Error, Result, TapType,
    MODEL_V2,
};

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
    pub fn new(i2c_addr: u16) -> Result<Self> {
        let mut i2c = I2c::new()?;
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
    pub fn init_auto_shutdown(&self) -> Result<()> {
        let threshold = PI_ZERO_IDLE_INTENSITY * 1000.0;
        let threshold = (threshold / 12.0) as u64;
        let threshold = if threshold > 0b0001_1111 {
            0b0001_1111 as u8
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
        v |= 0b1101_1111;
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

    /// Enable/Disable charging, 2 led version
    pub fn toggle_charging_2led(&self, enable: bool) -> Result<()> {
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

    /// Check charging
    pub fn is_charging_2led(&self) -> Result<bool> {
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
        // force shutdown
        let mut t = self.i2c.smbus_read_byte(0x01)?;
        t &= 0b1111_1011;
        self.i2c.smbus_write_byte(0x01, t)?;

        Ok(())
    }
}

pub struct IP5209Battery {
    ip5209: IP5209,
    led_amount: u32,
    voltages: VecDeque<(Instant, f32)>,
    intensities: VecDeque<(Instant, f32)>,
    tap_history: String,
}

impl IP5209Battery {
    pub fn new(i2c_addr: u16, led_amount: u32) -> Result<Self> {
        let ip5209 = IP5209::new(i2c_addr)?;
        let voltages = VecDeque::with_capacity(10);
        let intensities = VecDeque::with_capacity(10);
        let tap_history = String::with_capacity(100);
        Ok(Self {
            ip5209,
            led_amount,
            voltages,
            intensities,
            tap_history,
        })
    }
}

impl Battery for IP5209Battery {
    fn init(&mut self) -> Result<()> {
        if self.led_amount == 2 {
            self.ip5209.init_gpio_2led()?;
            self.ip5209.toggle_charging_2led(true)?;
        } else {
            self.ip5209.init_gpio()?;
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
        return MODEL_V2.to_string();
    }

    fn led_amount(&self) -> Result<u32> {
        Ok(self.led_amount)
    }

    fn voltage(&self) -> Result<f32> {
        self.ip5209.read_voltage().and_then(|v| Ok(v as f32))
    }

    fn voltage_avg(&self) -> Result<f32> {
        let mut total = 0.0;
        self.voltages.iter().for_each(|v| total += v.1);
        if self.voltages.len() > 0 {
            Ok(total / self.voltages.len() as f32)
        } else {
            Err(Error::Other("Required initialization".to_string()))
        }
    }

    fn level(&self) -> Result<f32> {
        self.voltage()
            .and_then(|x| Ok(IP5209::parse_voltage_level(x as f64) as f32))
    }

    fn intensity(&self) -> Result<f32> {
        self.ip5209.read_intensity().and_then(|x| Ok(x as f32))
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
            self.ip5209.is_charging_2led()
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
            self.ip5209.toggle_charging_2led(enable)
        } else {
            Err(Error::Other("Not supported".to_string()))
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

        let gpio_value = self.ip5209.read_gpio_tap()?;
        let tapped = if self.led_amount == 2 {
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
        Ok(tap_result)
    }

    fn shutdown(&self) -> Result<()> {
        self.ip5209.force_shutdown()
    }
}
