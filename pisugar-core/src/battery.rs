use crate::{Result, TapType};
use std::time::Instant;

/// Battery chip controller
pub trait Battery {
    /// Init battery chip
    fn init(&mut self) -> Result<()>;

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

    /// Is Battery charging
    fn is_charging(&self) -> Result<bool>;

    /// Enable/disable charging
    fn toggle_charging(&self, enable: bool) -> Result<()>;

    /// Poll and check tapped
    fn poll(&mut self, now: Instant) -> Result<Option<TapType>>;

    /// Shutdown battery chip
    fn shutdown(&self) -> Result<()>;
}
