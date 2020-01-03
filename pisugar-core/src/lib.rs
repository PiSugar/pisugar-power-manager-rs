#![allow(unused)]

const TIME_HOST: &str = "cdn.pisugar.com";

const RTC_ADDRESS: u8 = 0x32;
const BAT_ADDRESS: u8 = 0x75;

const CTR1: u8 = 0x0f;
const CTR2: u8 = 0x10;
const CTR3: u8 = 0x11;

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

/// Battery volatage to percentage
pub fn battery_voltage_percentage(voltage: f64) -> f64 {
    if voltage > 5.5 {
        return 100.0;
    }
    for threshold in &BATTERY_CURVE {
        if voltage >= threshold.0 && voltage < threshold.1 {
            let mut percentage = (voltage - threshold.0) / (threshold.1 - threshold.0);
            percentage *= (threshold.3 - threshold.2);
            percentage += threshold.2 + percentage;
            return percentage;
        }
    }
    0.0
}

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

pub struct PiSugarCore {
    config: PiSugarConfig,
}

impl PiSugarCore {
    pub fn new(config: PiSugarConfig) -> Self {
        Self { config }
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
}
