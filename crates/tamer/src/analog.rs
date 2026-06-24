//! Analog input helpers for ADC-backed controls.
//!
//! Analog peripherals usually produce raw ADC counts, while application code
//! wants a stable, range-independent value.
//! This module keeps that conversion pure and host-testable: callers feed raw
//! samples, [`AnalogRange`] clamps and normalizes them, and [`AnalogValue`]
//! reports only meaningful movement across a configured deadband.
//!
//! # Example
//!
//! ```
//! use tamer::analog::{AnalogRange, AnalogValue};
//!
//! let range = AnalogRange::new(0, 4095);
//! let deadband = range.raw_delta_to_normalized(32);
//! let mut poti = AnalogValue::new(0, range, deadband);
//!
//! assert_eq!(poti.update(10), None);
//! assert_eq!(poti.update(128), Some(poti.stable_value()));
//! assert_eq!(poti.stable_value().percent(), 3);
//! ```

use core::convert::Infallible;

/// The maximum normalized analog value.
pub const ANALOG_FULL_SCALE: u16 = u16::MAX;

/// A raw ADC range used to normalize samples.
///
/// Samples below `min` clamp to `0`.
/// Samples above `max` clamp to [`ANALOG_FULL_SCALE`].
/// The range must have at least one count of span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnalogRange {
    min: u16,
    max: u16,
}

impl AnalogRange {
    /// Creates a new raw ADC range.
    ///
    /// # Panics
    ///
    /// Panics if `max <= min`.
    #[must_use]
    pub const fn new(min: u16, max: u16) -> Self {
        assert!(max > min, "analog range max must be greater than min");
        Self { min, max }
    }

    /// Creates a range for an unsigned ADC with the given inclusive maximum.
    ///
    /// For a 12-bit ADC, pass `4095`.
    ///
    /// # Panics
    ///
    /// Panics if `max == 0`.
    #[must_use]
    pub const fn zero_to(max: u16) -> Self {
        Self::new(0, max)
    }

    /// Returns the inclusive lower raw bound.
    #[must_use]
    pub const fn min(self) -> u16 {
        self.min
    }

    /// Returns the inclusive upper raw bound.
    #[must_use]
    pub const fn max(self) -> u16 {
        self.max
    }

    /// Returns the raw span (`max - min`).
    #[must_use]
    pub const fn span(self) -> u16 {
        self.max - self.min
    }

    /// Clamps a raw sample to this range.
    #[must_use]
    pub const fn clamp(self, raw: u16) -> u16 {
        if raw < self.min {
            self.min
        } else if raw > self.max {
            self.max
        } else {
            raw
        }
    }

    /// Normalizes a raw sample to `0..=u16::MAX`.
    #[must_use]
    pub fn normalize(self, raw: u16) -> u16 {
        let clamped = self.clamp(raw);
        let offset = u32::from(clamped - self.min);
        let span = u32::from(self.span());

        ((offset * u32::from(ANALOG_FULL_SCALE) + (span / 2)) / span) as u16
    }

    /// Converts a raw ADC delta to normalized units for this range.
    ///
    /// This is useful for configuring [`AnalogValue`] deadbands in the same
    /// units as a datasheet or observed ADC jitter.
    /// A `raw_delta` of `0` maps to `0`.
    /// Any delta greater than or equal to the range span maps to
    /// [`ANALOG_FULL_SCALE`].
    #[must_use]
    pub fn raw_delta_to_normalized(self, raw_delta: u16) -> u16 {
        if raw_delta == 0 {
            return 0;
        }

        if raw_delta >= self.span() {
            return ANALOG_FULL_SCALE;
        }

        let delta = u32::from(raw_delta);
        let span = u32::from(self.span());

        ((delta * u32::from(ANALOG_FULL_SCALE) + (span / 2)) / span) as u16
    }
}

/// A normalized analog reading with the raw clamped sample that produced it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AnalogSample {
    raw: u16,
    normalized: u16,
}

impl AnalogSample {
    /// Creates a sample from raw and normalized values.
    #[must_use]
    pub const fn new(raw: u16, normalized: u16) -> Self {
        Self { raw, normalized }
    }

    /// Returns the clamped raw ADC sample.
    #[must_use]
    pub const fn raw(self) -> u16 {
        self.raw
    }

    /// Returns the normalized value in `0..=u16::MAX`.
    #[must_use]
    pub const fn normalized(self) -> u16 {
        self.normalized
    }

    /// Returns an integer percentage in `0..=100`.
    #[must_use]
    pub fn percent(self) -> u8 {
        let full = u32::from(ANALOG_FULL_SCALE);

        ((u32::from(self.normalized) * 100 + (full / 2)) / full) as u8
    }
}

/// Deadbanded analog value tracker.
///
/// `AnalogValue` keeps the latest stable normalized value and emits a new
/// [`AnalogSample`] only when the normalized movement is at least `deadband`.
/// The deadband is expressed in normalized units (`0..=u16::MAX`), so it is
/// independent of the ADC resolution.
/// This is change-threshold emission, not averaging or smoothing.
/// Jitter that stays inside the deadband around the last emitted value is
/// suppressed, while a sample that crosses the threshold becomes the new stable
/// comparison point.
#[derive(Debug, Clone, Copy)]
pub struct AnalogValue {
    range: AnalogRange,
    deadband: u16,
    stable: AnalogSample,
}

impl AnalogValue {
    /// Creates a new tracker seeded from an initial raw ADC sample.
    #[must_use]
    pub fn new(initial_raw: u16, range: AnalogRange, deadband: u16) -> Self {
        let stable = Self::sample(range, initial_raw);
        Self {
            range,
            deadband,
            stable,
        }
    }

    /// Feeds a raw ADC sample.
    ///
    /// Returns `Some(sample)` when the normalized value moved by at least the
    /// configured deadband.
    /// Returns `None` otherwise.
    ///
    /// With a deadband of `0`, any normalized change is emitted.
    pub fn update(&mut self, raw: u16) -> Option<AnalogSample> {
        let sample = Self::sample(self.range, raw);
        let delta = sample.normalized.abs_diff(self.stable.normalized);

        if delta >= self.deadband && sample.normalized != self.stable.normalized {
            self.stable = sample;
            Some(sample)
        } else {
            None
        }
    }

    /// Returns the current stable sample.
    #[must_use]
    pub const fn stable_value(&self) -> AnalogSample {
        self.stable
    }

    /// Returns the configured raw ADC range.
    #[must_use]
    pub const fn range(&self) -> AnalogRange {
        self.range
    }

    /// Returns the configured normalized deadband.
    #[must_use]
    pub const fn deadband(&self) -> u16 {
        self.deadband
    }

    fn sample(range: AnalogRange, raw: u16) -> AnalogSample {
        let clamped = range.clamp(raw);
        AnalogSample::new(clamped, range.normalize(clamped))
    }
}

/// Minimal trait for ADC-like raw analog readers.
///
/// The pure logic in this module does not depend on a specific hardware HAL.
/// Hardware crates and applications can implement this trait for their own ADC
/// wrappers, while tests can use [`MockAnalogRead`].
pub trait AnalogRead {
    /// The read error type.
    type Error;

    /// Reads one raw ADC sample.
    fn read_raw(&mut self) -> Result<u16, Self::Error>;
}

/// Adapter that reads raw samples and feeds an [`AnalogValue`] tracker.
pub struct AnalogInput<R> {
    reader: R,
    value: AnalogValue,
}

impl<R: AnalogRead> AnalogInput<R> {
    /// Creates a new analog input from an already-known initial raw sample.
    #[must_use]
    pub fn new(reader: R, initial_raw: u16, range: AnalogRange, deadband: u16) -> Self {
        Self {
            reader,
            value: AnalogValue::new(initial_raw, range, deadband),
        }
    }

    /// Creates a new analog input, seeding the initial value from the reader.
    ///
    /// # Errors
    ///
    /// Returns `Err(e)` if the initial read fails.
    pub fn try_from_reader(
        mut reader: R,
        range: AnalogRange,
        deadband: u16,
    ) -> Result<Self, R::Error> {
        let initial_raw = reader.read_raw()?;
        Ok(Self::new(reader, initial_raw, range, deadband))
    }

    /// Reads one raw sample and updates the deadbanded value.
    ///
    /// # Errors
    ///
    /// Returns `Err(e)` if the raw read fails.
    pub fn update(&mut self) -> Result<Option<AnalogSample>, R::Error> {
        let raw = self.reader.read_raw()?;
        Ok(self.value.update(raw))
    }

    /// Returns the current stable sample.
    #[must_use]
    pub const fn stable_value(&self) -> AnalogSample {
        self.value.stable_value()
    }

    /// Returns an immutable reference to the wrapped reader.
    #[must_use]
    pub const fn reader(&self) -> &R {
        &self.reader
    }

    /// Returns a mutable reference to the wrapped reader.
    #[must_use]
    pub fn reader_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Releases the wrapped reader and value tracker.
    #[must_use]
    pub fn into_parts(self) -> (R, AnalogValue) {
        (self.reader, self.value)
    }
}

/// Settable mock analog reader for host tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MockAnalogRead {
    raw: u16,
}

impl MockAnalogRead {
    /// Creates a mock reader with the given raw sample.
    #[must_use]
    pub const fn new(raw: u16) -> Self {
        Self { raw }
    }

    /// Sets the raw sample returned by future reads.
    pub fn set_raw(&mut self, raw: u16) {
        self.raw = raw;
    }

    /// Returns the currently configured raw sample.
    #[must_use]
    pub const fn raw(self) -> u16 {
        self.raw
    }
}

impl AnalogRead for MockAnalogRead {
    type Error = Infallible;

    fn read_raw(&mut self) -> Result<u16, Self::Error> {
        Ok(self.raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_normalizes_bounds() {
        let range = AnalogRange::zero_to(4095);

        assert_eq!(range.normalize(0), 0);
        assert_eq!(range.normalize(4095), ANALOG_FULL_SCALE);
    }

    #[test]
    fn range_clamps_out_of_bounds_samples() {
        let range = AnalogRange::new(100, 1100);

        assert_eq!(range.clamp(0), 100);
        assert_eq!(range.clamp(1200), 1100);
        assert_eq!(range.normalize(0), 0);
        assert_eq!(range.normalize(1200), ANALOG_FULL_SCALE);
    }

    #[test]
    fn range_normalizes_midpoint() {
        let range = AnalogRange::zero_to(1023);

        assert_eq!(range.normalize(512), 32800);
    }

    #[test]
    fn sample_reports_percent() {
        assert_eq!(AnalogSample::new(0, 0).percent(), 0);
        assert_eq!(AnalogSample::new(0, ANALOG_FULL_SCALE / 2).percent(), 50);
        assert_eq!(AnalogSample::new(0, ANALOG_FULL_SCALE).percent(), 100);
    }

    #[test]
    fn raw_delta_to_normalized_converts_adc_counts() {
        let range = AnalogRange::zero_to(4095);

        assert_eq!(range.raw_delta_to_normalized(0), 0);
        assert_eq!(range.raw_delta_to_normalized(32), 512);
        assert_eq!(range.raw_delta_to_normalized(4095), ANALOG_FULL_SCALE);
    }

    #[test]
    fn value_ignores_movement_inside_deadband() {
        let mut value = AnalogValue::new(0, AnalogRange::zero_to(4095), 512);

        assert_eq!(value.update(10), None);
        assert_eq!(value.stable_value().raw(), 0);
    }

    #[test]
    fn value_emits_movement_at_deadband() {
        let mut value = AnalogValue::new(0, AnalogRange::zero_to(4095), 512);

        let sample = value.update(32).unwrap();

        assert_eq!(sample.raw(), 32);
        assert_eq!(value.stable_value(), sample);
    }

    #[test]
    fn zero_deadband_emits_any_normalized_change() {
        let mut value = AnalogValue::new(0, AnalogRange::zero_to(4095), 0);

        assert_eq!(value.update(0), None);
        assert_eq!(value.update(1).unwrap().raw(), 1);
    }

    #[test]
    fn input_seeds_from_reader_and_updates() {
        let reader = MockAnalogRead::new(0);
        let mut input =
            AnalogInput::try_from_reader(reader, AnalogRange::zero_to(4095), 512).unwrap();

        input.reader_mut().set_raw(32);

        assert_eq!(input.update().unwrap().unwrap().raw(), 32);
    }
}
