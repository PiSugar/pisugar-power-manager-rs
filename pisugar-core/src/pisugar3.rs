use crate::battery::Battery;
use crate::ip5312::IP5312;
use crate::rtc::RTC;
use crate::{Error, Model, RTCRawTime, Result, TapType};
use chrono::{DateTime, Local};
use rppal::i2c::I2c;
use std::collections::VecDeque;
use std::time::Instant;

/// PiSugar 3 i2c addr
pub const I2C_ADDR_P3: u16 = 0x57;

/// Global ctrl 1
const IIC_CMD_CTR1: u8 = 0x02;

/// Battery ctrl
const IIC_CMD_BAT_CTR: u8 = 0x20;

/// Voltage high byte
const IIC_CMD_VH: u8 = 0x22;
/// Voltage low byte
const IIC_CMD_VL: u8 = 0x23;

/// input current High byte
const IIC_CMD_IH: u8 = 0x24;
/// Input current low byte
const IIC_CMD_IL: u8 = 0x25;
/// Output current high byte
const IIC_CMD_OH: u8 = 0x26;
/// Output current lob byte
const IIC_CMD_OL: u8 = 0x27;

/// Under load current 0-255mA
const IIC_CMD_UL_C: u8 = 0x28;
/// Under load delay (*2s)
const IIC_CMD_UL_D: u8 = 0x29;

/// Battery percent
const IIC_CMD_P: u8 = 0x2A;

/// PiSugar 3
pub struct PiSugar3 {
    i2c: I2c,
}

impl PiSugar3 {
    pub fn new(i2c_bus: u8, i2c_addr: u16) -> Result<Self> {
        let mut i2c = I2c::with_bus(i2c_bus)?;
        i2c.set_slave_address(i2c_addr)?;
        Ok(Self { i2c })
    }

    pub fn read_ctr1(&self) -> Result<u8> {
        let ctr1 = self.i2c.smbus_read_byte(IIC_CMD_CTR1)?;
        Ok(ctr1)
    }

    pub fn write_ctr1(&self, ctr1: u8) -> Result<()> {
        self.i2c.smbus_write_byte(IIC_CMD_CTR1, ctr1)?;
        Ok(())
    }

    pub fn read_bat_ctr(&self) -> Result<u8> {
        let ctr = self.i2c.smbus_read_byte(IIC_CMD_BAT_CTR)?;
        Ok(ctr)
    }

    pub fn write_bat_ctr(&self, ctr: u8) -> Result<()> {
        self.i2c.smbus_write_byte(IIC_CMD_BAT_CTR, ctr)?;
        Ok(())
    }

    pub fn read_voltage(&self) -> Result<u16> {
        let vh: u16 = self.i2c.smbus_read_byte(IIC_CMD_VH)? as u16;
        let vl: u16 = self.i2c.smbus_read_byte(IIC_CMD_VL)? as u16;
        let v = (vh << 8) | vl;
        Ok(v)
    }

    pub fn read_percent(&self) -> Result<u8> {
        let p = self.i2c.smbus_read_byte(IIC_CMD_P)?;
        Ok(p)
    }

    pub fn read_output_current(&self) -> Result<u16> {
        let oh: u16 = self.i2c.smbus_read_byte(IIC_CMD_OH)? as u16;
        let ol: u16 = self.i2c.smbus_read_byte(IIC_CMD_OL)? as u16;
        let oc = (oh << 8) | ol;
        Ok(oc)
    }
}

/// PiSugar 3 Battery support
pub struct PiSugar3Battery {
    pisugar3: PiSugar3,
    model: Model,
    voltages: VecDeque<(Instant, f32)>,
    intensities: VecDeque<(Instant, f32)>,
    levels: VecDeque<f32>,
}

impl PiSugar3Battery {
    pub fn new(i2c_bus: u8, i2c_addr: u16, model: Model) -> Result<Self> {
        let pisugar3 = PiSugar3::new(i2c_bus, i2c_addr)?;
        Ok(Self {
            pisugar3,
            model,
            voltages: VecDeque::with_capacity(30),
            intensities: VecDeque::with_capacity(30),
            levels: VecDeque::with_capacity(30),
        })
    }
}

impl Battery for PiSugar3Battery {
    fn init(&mut self, auto_power_on: bool) -> crate::Result<()> {
        todo!()
    }

    fn model(&self) -> String {
        self.model.to_string()
    }

    fn led_amount(&self) -> crate::Result<u32> {
        Ok(self.model.led_amount())
    }

    fn voltage(&self) -> crate::Result<f32> {
        let v = self.pisugar3.read_voltage()?;
        Ok((v as f32) / (1000 as f32))
    }

    fn voltage_avg(&self) -> crate::Result<f32> {
        let mut total = 0.0;
        self.voltages.iter().for_each(|v| total += v.1);
        if self.voltages.len() > 0 {
            Ok(total / self.voltages.len() as f32)
        } else {
            Err(Error::Other("Require initialization".to_string()))
        }
    }

    fn level(&self) -> crate::Result<f32> {
        self.voltage_avg().and_then(|v| Ok(IP5312::parse_voltage_level(v)))
    }

    fn intensity(&self) -> crate::Result<f32> {
        let c = self.pisugar3.read_output_current()?;
        Ok((c as f32) / 1000 as f32)
    }

    fn intensity_avg(&self) -> crate::Result<f32> {
        let mut total = 0.0;
        self.intensities.iter().for_each(|i| total += i.1);
        if self.intensities.len() > 0 {
            Ok(total / self.intensities.len() as f32)
        } else {
            Err(Error::Other("Require initialization".to_string()))
        }
    }

    fn is_power_plugged(&self) -> crate::Result<bool> {
        let ctr1 = self.pisugar3.read_ctr1()?;
        Ok((ctr1 & (1 << 7)) != 0)
    }

    fn is_allow_charging(&self) -> crate::Result<bool> {
        let ctr1 = self.pisugar3.read_ctr1()?;
        Ok((ctr1 & (1 << 6)) != 0)
    }

    fn toggle_allow_charging(&self, enable: bool) -> crate::Result<()> {
        let mut ctr1 = self.pisugar3.read_ctr1()?;
        ctr1 &= 0b1011_1111;
        if enable {
            ctr1 |= 0b0100_0000;
        }
        self.pisugar3.write_ctr1(ctr1)
    }

    fn is_charging(&self) -> crate::Result<bool> {
        let power_plugged = self.is_power_plugged()?;
        let allow_charging = self.is_allow_charging()?;
        return Ok(power_plugged && allow_charging);
    }

    fn poll(&mut self, now: Instant) -> crate::Result<Option<TapType>> {
        // PiSugar 3 doesn't support tap
        Ok(None)
    }

    fn shutdown(&self) -> crate::Result<()> {
        let mut ctr1 = self.pisugar3.read_ctr1()?;
        ctr1 &= 0b1101_1111;
        self.pisugar3.write_ctr1(ctr1)
    }

    fn toggle_light_load_shutdown(&self, enable: bool) -> crate::Result<()> {
        let mut bat_ctr = self.pisugar3.read_bat_ctr()?;
        bat_ctr &= 0b1101_1111;
        if enable {
            bat_ctr |= 0b0010_0000;
        }
        self.pisugar3.write_bat_ctr(bat_ctr)?;
        Ok(())
    }
}

pub struct PiSugar3RTC {
    pisugar3: PiSugar3,
}

impl PiSugar3RTC {
    pub fn new(i2c_bus: u8, i2c_addr: u16) -> Result<Self> {
        let pisugar3 = PiSugar3::new(i2c_bus, i2c_addr)?;
        Ok(Self { pisugar3 })
    }
}

impl RTC for PiSugar3RTC {
    fn init(&self, auto_power_on: bool, auto_wakeup_time: Option<DateTime<Local>>, wakeup_repeat: u8) -> Result<()> {
        if auto_power_on {
            //self.pisugar3.toggle_restore(true);
        }
        Ok(())
    }

    fn read_time(&self) -> Result<RTCRawTime> {
        todo!()
    }

    fn write_time(&self, time: RTCRawTime) -> Result<()> {
        todo!()
    }

    fn read_alarm_time(&self) -> Result<RTCRawTime> {
        todo!()
    }

    fn set_alarm(&self, time: RTCRawTime, weekday_repeat: u8) -> Result<()> {
        todo!()
    }

    fn is_alarm_enable(&self) -> Result<bool> {
        todo!()
    }

    fn toggle_alarm_enable(&self, enable: bool) -> Result<()> {
        todo!()
    }

    fn read_alarm_flag(&self) -> Result<bool> {
        todo!()
    }

    fn clear_alarm_flag(&self) -> Result<()> {
        todo!()
    }

    fn toggle_frequency_alarm(&self, enable: bool) -> Result<()> {
        todo!()
    }

    fn set_test_wake(&self) -> Result<()> {
        todo!()
    }

    fn force_shutdown(&self) -> Result<()> {
        todo!()
    }

    fn read_battery_low_flag(&self) -> Result<bool> {
        todo!()
    }

    fn toggle_charging(&self, enable: bool) -> Result<()> {
        todo!()
    }

    fn read_battery_high_flag(&self) -> Result<bool> {
        todo!()
    }
}
