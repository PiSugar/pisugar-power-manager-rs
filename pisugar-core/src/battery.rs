use std::collections::VecDeque;
use std::time::Instant;

use crate::{Result, TapType};

/// Battery chip controller
pub trait Battery {
    /// Init battery chip
    fn init(&mut self, auto_power_on: bool) -> Result<()>;

    /// Model
    fn model(&self) -> String;

    /// LED amount
    fn led_amount(&self) -> Result<u32>;

    /// Battery voltage (V)
    fn voltage(&self) -> Result<f32>;

    /// Battery average voltage (V)
    fn voltage_avg(&self) -> Result<f32>;

    /// Battery voltage level
    fn level(&self) -> Result<f32>;

    /// Battery current intensity (A)
    fn intensity(&self) -> Result<f32>;

    /// Battery average current intensity (A)
    fn intensity_avg(&self) -> Result<f32>;

    /// Is power cable plugged in
    fn is_power_plugged(&self) -> Result<bool>;

    /// Is battery allow charging
    fn is_allow_charging(&self) -> Result<bool>;

    /// Enable/disable charging
    fn toggle_allow_charging(&self, enable: bool) -> Result<()>;

    /// Is battery charging
    fn is_charging(&self) -> Result<bool>;

    /// Is input protect enabled
    fn is_input_protected(&self) -> Result<bool>;

    /// Toggle input protect
    fn toggle_input_protected(&self, enable: bool) -> Result<()>;

    /// Output enabled
    fn output_enabled(&self) -> Result<bool>;

    /// Toggle output enable
    fn toggle_output_enabled(&self, enable: bool) -> Result<()>;

    /// Poll and check tapped
    fn poll(&mut self, now: Instant) -> Result<Option<TapType>>;

    /// Shutdown battery chip, call `toggle_output_enabled(false)`
    fn shutdown(&self) -> Result<()> {
        self.toggle_output_enabled(false)
    }

    /// Enable/disable light load shutdown
    fn toggle_light_load_shutdown(&self, enable: bool) -> Result<()>;

    /// Toggle soft poweroff
    fn toggle_soft_poweroff(&self, enable: bool) -> Result<()>;
}

#[allow(dead_code)]
pub fn check_charging(levels: &VecDeque<f32>) -> bool {
    let capacity = levels.len() as f32;
    let x_sum = (0.0 + capacity - 1.0) * capacity / 2.0;
    let x_bar = x_sum / capacity;
    let y_sum: f32 = levels.iter().sum();
    let _y_bar = y_sum / capacity;
    // k = Sum(yi * (xi - x_bar)) / Sum(xi - x_bar)^2
    let mut iter = levels.iter();
    let mut a = 0.0;
    let mut b = 0.0;
    for i in 0..levels.len() {
        let xi = i as f32;
        let yi = iter.next().unwrap().clone();
        a += yi * (xi - x_bar);
        b += (xi - x_bar) * (xi - x_bar);
    }
    let k = a / b;
    log::debug!("Charging k: {}", k);
    return k >= 0.005;
}
