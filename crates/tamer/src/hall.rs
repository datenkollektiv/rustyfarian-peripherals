//! Hall-effect magnetic presence detection.
//!
//! [`HallSensor`] is a pure, HAL-agnostic threshold evaluator for linear
//! Hall-effect sensors.
//! It compares the absolute deviation of a raw ADC reading from a calibrated
//! midpoint against a configurable threshold, and emits [`Presence::Present`]
//! or [`Presence::Absent`] accordingly.
//!
//! Pair it with [`crate::smoothing::SlidingAverage`] to absorb ADC
//! quantization noise before evaluation.
//!
//! # Example
//!
//! ```
//! use tamer::hall::HallSensor;
//! use tamer::presence::Presence;
//!
//! let mut sensor = HallSensor::new(300, 2048);
//!
//! // Calibrate midpoint from no-magnet samples.
//! sensor.calibrate_from_samples(&[2040, 2055, 2050]).unwrap();
//!
//! // Magnet present — reading deviates beyond threshold.
//! assert_eq!(sensor.evaluate(2400), Presence::Present);
//!
//! // No magnet — reading near midpoint.
//! assert_eq!(sensor.evaluate(2060), Presence::Absent);
//! ```

use crate::presence::Presence;

/// Error type returned by [`HallSensor::calibrate_from_samples`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HallCalibrationError {
    /// Calibration was requested with an empty sample slice.
    EmptySamples,
}

impl core::fmt::Display for HallCalibrationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HallCalibrationError::EmptySamples => {
                f.write_str("calibration requires at least one sample")
            }
        }
    }
}

/// Pure threshold-based presence detector for a linear Hall-effect sensor.
///
/// Compares the absolute deviation of a raw ADC reading from a calibrated
/// midpoint against a configurable threshold.
/// Deviation at or above threshold returns [`Presence::Present`]; below
/// threshold returns [`Presence::Absent`].
///
/// This type contains no hardware dependencies — it operates on raw
/// unsigned ADC values provided by the caller.
/// Pair it with [`crate::smoothing::SlidingAverage`] to absorb ADC
/// quantization noise before calling [`evaluate`](Self::evaluate).
///
/// # Why a single threshold, not hysteresis
///
/// v1 intentionally uses a single absolute-deviation threshold rather than
/// separate rising/falling thresholds.
/// For noisy production deployments, pair this with
/// [`crate::smoothing::SlidingAverage`] to damp ADC quantization noise and
/// reduce chatter near the boundary.
/// Hysteresis is a plausible future addition for sensors that hover right at
/// the threshold in production, but it is not implemented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HallSensor {
    midpoint: u16,
    threshold: u16,
}

impl HallSensor {
    /// Creates a new evaluator with the given presence threshold and midpoint.
    ///
    /// `threshold` is the minimum absolute deviation from `midpoint` required
    /// to report [`Presence::Present`].
    /// `midpoint` is the expected ADC reading with no magnet present
    /// (typically `2048` for a 12-bit ADC centered at VCC/2).
    #[must_use]
    pub const fn new(threshold: u16, midpoint: u16) -> Self {
        Self {
            midpoint,
            threshold,
        }
    }

    /// Returns the configured presence threshold.
    ///
    /// This is the minimum absolute deviation from [`midpoint`](Self::midpoint)
    /// required to report [`Presence::Present`].
    #[must_use]
    pub const fn threshold(&self) -> u16 {
        self.threshold
    }

    /// Returns the current no-magnet midpoint.
    #[must_use]
    pub const fn midpoint(&self) -> u16 {
        self.midpoint
    }

    /// Sets the midpoint directly, overriding any previous calibration.
    pub fn set_midpoint(&mut self, midpoint: u16) {
        self.midpoint = midpoint;
    }

    /// Sets the presence threshold directly, overriding the value from
    /// [`new`](Self::new).
    ///
    /// Useful during bring-up to tune sensitivity without reconstructing the
    /// sensor; a lower threshold detects weaker fields.
    pub fn set_threshold(&mut self, threshold: u16) {
        self.threshold = threshold;
    }

    /// Calibrates the midpoint by averaging the provided no-magnet samples.
    ///
    /// After a successful call, [`evaluate`](Self::evaluate) compares
    /// readings relative to this new baseline.
    /// The midpoint is left **unchanged** when an error is returned.
    ///
    /// # Errors
    ///
    /// Returns [`HallCalibrationError::EmptySamples`] if `samples` is empty.
    pub fn calibrate_from_samples(&mut self, samples: &[u16]) -> Result<(), HallCalibrationError> {
        if samples.is_empty() {
            return Err(HallCalibrationError::EmptySamples);
        }
        let sum: u32 = samples.iter().map(|&s| u32::from(s)).sum();
        self.midpoint = (sum / samples.len() as u32) as u16;
        Ok(())
    }

    /// Evaluates a raw ADC reading against the threshold.
    ///
    /// Returns [`Presence::Present`] if the absolute deviation from
    /// [`midpoint`](Self::midpoint) is at or above the configured threshold,
    /// [`Presence::Absent`] otherwise.
    #[must_use]
    pub const fn evaluate(&self, raw: u16) -> Presence {
        if self.deviation(raw) >= self.threshold {
            Presence::Present
        } else {
            Presence::Absent
        }
    }

    /// Returns the absolute deviation of `raw` from the midpoint.
    ///
    /// Useful for diagnostics and threshold-tuning during development.
    #[must_use]
    pub const fn deviation(&self, raw: u16) -> u16 {
        raw.abs_diff(self.midpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_midpoint_and_threshold() {
        let sensor = HallSensor::new(300, 2048);

        assert_eq!(sensor.midpoint(), 2048);
        assert_eq!(sensor.threshold(), 300);
    }

    #[test]
    fn calibrate_averages_samples() {
        let mut sensor = HallSensor::new(300, 0);

        sensor.calibrate_from_samples(&[2040, 2050, 2060]).unwrap();

        assert_eq!(sensor.midpoint(), 2050);
    }

    #[test]
    fn calibrate_single_sample() {
        let mut sensor = HallSensor::new(300, 0);

        sensor.calibrate_from_samples(&[2048]).unwrap();

        assert_eq!(sensor.midpoint(), 2048);
    }

    #[test]
    fn calibrate_empty_samples_returns_err() {
        let mut sensor = HallSensor::new(300, 2048);

        assert_eq!(
            sensor.calibrate_from_samples(&[]),
            Err(HallCalibrationError::EmptySamples)
        );
    }

    #[test]
    fn calibrate_midpoint_unchanged_on_error() {
        let mut sensor = HallSensor::new(300, 2048);

        let _ = sensor.calibrate_from_samples(&[]);

        assert_eq!(sensor.midpoint(), 2048);
    }

    #[test]
    fn hall_calibration_error_display() {
        let err = HallCalibrationError::EmptySamples;

        assert_eq!(err.to_string(), "calibration requires at least one sample");
    }

    #[test]
    fn set_midpoint_overrides() {
        let mut sensor = HallSensor::new(300, 2048);
        sensor.calibrate_from_samples(&[2040, 2050, 2060]).unwrap();

        sensor.set_midpoint(1000);

        assert_eq!(sensor.midpoint(), 1000);
    }

    #[test]
    fn set_threshold_overrides() {
        let mut sensor = HallSensor::new(300, 2048);

        sensor.set_threshold(100);

        assert_eq!(sensor.threshold(), 100);
        // deviation from 2048: 2150 - 2048 = 102 >= 100 → Present
        // (would have been Absent under the original threshold of 300)
        assert_eq!(sensor.evaluate(2150), Presence::Present);
    }

    #[test]
    fn evaluate_after_set_midpoint() {
        let mut sensor = HallSensor::new(100, 2048);
        sensor.set_midpoint(3000);

        // deviation from 3000: 3150 - 3000 = 150 >= 100 → Present
        assert_eq!(sensor.evaluate(3150), Presence::Present);
        // deviation from 3000: 3050 - 3000 = 50 < 100 → Absent
        assert_eq!(sensor.evaluate(3050), Presence::Absent);
    }

    #[test]
    fn evaluate_present_above_midpoint() {
        let sensor = HallSensor::new(300, 2048);

        // raw=2400, deviation=352 >= 300 → Present
        assert_eq!(sensor.evaluate(2400), Presence::Present);
    }

    #[test]
    fn evaluate_present_below_midpoint() {
        let sensor = HallSensor::new(300, 2048);

        // raw=1700, deviation=348 >= 300 → Present
        assert_eq!(sensor.evaluate(1700), Presence::Present);
    }

    #[test]
    fn evaluate_absent_near_midpoint() {
        let sensor = HallSensor::new(300, 2048);

        // raw=2100, deviation=52 < 300 → Absent
        assert_eq!(sensor.evaluate(2100), Presence::Absent);
    }

    #[test]
    fn evaluate_present_at_exact_threshold() {
        let sensor = HallSensor::new(300, 2048);

        // raw=2348, deviation=300 >= 300 → Present
        assert_eq!(sensor.evaluate(2348), Presence::Present);
    }

    #[test]
    fn evaluate_absent_just_below_threshold() {
        let sensor = HallSensor::new(300, 2048);

        // raw=2347, deviation=299 < 300 → Absent
        assert_eq!(sensor.evaluate(2347), Presence::Absent);
    }

    #[test]
    fn evaluate_present_at_extremes() {
        let sensor = HallSensor::new(300, 2048);

        assert_eq!(sensor.evaluate(0), Presence::Present);
        assert_eq!(sensor.evaluate(4095), Presence::Present);
    }

    #[test]
    fn deviation_returns_absolute_difference() {
        let sensor = HallSensor::new(300, 2048);

        assert_eq!(sensor.deviation(2400), 352);
        assert_eq!(sensor.deviation(1700), 348);
        assert_eq!(sensor.deviation(2048), 0);
    }

    #[test]
    fn calibrate_handles_large_values() {
        let mut sensor = HallSensor::new(300, 0);

        // Values near u16::MAX — sum fits in u32.
        sensor.calibrate_from_samples(&[4095, 4095]).unwrap();

        assert_eq!(sensor.midpoint(), 4095);
    }

    #[test]
    fn typical_calibrated_workflow() {
        let mut sensor = HallSensor::new(300, 0);

        // Realistic no-magnet samples clustered around 2050.
        sensor
            .calibrate_from_samples(&[2048, 2050, 2052, 2049, 2051])
            .unwrap();
        assert_eq!(sensor.midpoint(), 2050);

        // raw=2100, deviation=|2100-2050|=50 < 300 → Absent (no magnet).
        assert_eq!(sensor.evaluate(2100), Presence::Absent);

        // raw=2400, deviation=|2400-2050|=350 >= 300 → Present (above midpoint).
        assert_eq!(sensor.evaluate(2400), Presence::Present);

        // raw=1700, deviation=|1700-2050|=350 >= 300 → Present (below midpoint).
        assert_eq!(sensor.evaluate(1700), Presence::Present);
    }
}
