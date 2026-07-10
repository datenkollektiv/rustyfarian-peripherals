//! Clamped linear remap from a `u16` analog reading to a `u8` output.
//!
//! [`RangeMap`] is the host-testable mapping step for analog→PWM auto-adjust
//! behavior — for example dimming a backlight from an LDR reading, after the
//! raw reading has been smoothed with [`crate::smoothing::SlidingAverage`] —
//! so that math lives in the pure core rather than as hand-rolled inline
//! arithmetic in a device example.
//!
//! # Example
//!
//! ```
//! use tamer::range_map::RangeMap;
//!
//! // Ambient-light LDR counts (12-bit ADC) -> LEDC backlight duty.
//! // Brighter room (higher counts) should dim the backlight, so invert.
//! let backlight = RangeMap::new(0, 4095, 0, 255).inverted();
//!
//! assert_eq!(backlight.map(0), 255); // dark room -> full brightness
//! assert_eq!(backlight.map(4095), 0); // bright room -> backlight off
//! ```

/// A clamped linear transfer function from a `u16` input to a `u8` output.
///
/// `RangeMap` clamps `reading` to `in_min..=in_max` and linearly scales it
/// onto `out_min..=out_max`, rounding to the nearest output count using the
/// same widened-intermediate rounding rule as
/// [`AnalogRange::normalize`](crate::analog::AnalogRange::normalize).
/// [`inverted`](Self::inverted) swaps the output endpoints, for controls
/// where a rising input should produce a falling output (e.g. brighter
/// ambient light → lower backlight duty).
///
/// `map` never panics; the only panic path is [`new`](Self::new) when
/// `in_min == in_max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RangeMap {
    in_min: u16,
    in_max: u16,
    out_min: u8,
    out_max: u8,
    inverted: bool,
}

impl RangeMap {
    /// Creates a new clamped linear range map.
    ///
    /// `reading`s passed to [`map`](Self::map) are clamped to
    /// `in_min..=in_max` before scaling onto `out_min..=out_max`.
    ///
    /// `out_min` and `out_max` are the outputs at `in_min` and `in_max`
    /// respectively. Passing `out_min > out_max` is supported and yields a
    /// descending map directly (a rising reading lowers the output); for that
    /// case prefer the more readable [`inverted`](Self::inverted), which swaps
    /// the endpoints of an ascending range.
    ///
    /// # Panics
    ///
    /// Panics if `in_min >= in_max`.
    #[must_use]
    pub const fn new(in_min: u16, in_max: u16, out_min: u8, out_max: u8) -> Self {
        assert!(in_min < in_max, "range map in_min must be less than in_max");
        Self {
            in_min,
            in_max,
            out_min,
            out_max,
            inverted: false,
        }
    }

    /// Returns an equivalent map with the output endpoints swapped.
    ///
    /// After inversion, `map(in_min) == out_max` and `map(in_max) == out_min`.
    /// Calling `inverted()` twice restores the original mapping.
    #[must_use]
    pub const fn inverted(self) -> Self {
        Self {
            inverted: !self.inverted,
            ..self
        }
    }

    /// Maps a raw reading through the clamped linear transfer function.
    ///
    /// `reading` is first clamped to `in_min..=in_max`, then linearly scaled
    /// onto `out_min..=out_max` (or `out_max..=out_min` when
    /// [`inverted`](Self::inverted) has been applied), rounding to the
    /// nearest output count.
    /// Endpoints are exact: `map(<= in_min)` returns the low output endpoint
    /// and `map(>= in_max)` returns the high output endpoint, with no
    /// rounding error.
    ///
    /// This function never panics for any `u16` reading.
    #[must_use]
    pub fn map(&self, reading: u16) -> u8 {
        let (start, end) = if self.inverted {
            (self.out_max, self.out_min)
        } else {
            (self.out_min, self.out_max)
        };

        let clamped = reading.clamp(self.in_min, self.in_max);
        let offset = u32::from(clamped - self.in_min);
        let in_span = u32::from(self.in_max - self.in_min);
        let out_span = i32::from(end) - i32::from(start);

        // Round-to-nearest via a widened intermediate, mirroring
        // `AnalogRange::normalize`: add `in_span / 2` before dividing so the
        // result rounds instead of truncating toward zero. `out_span` can be
        // negative (inversion), so the numerator and division use the
        // absolute magnitude and the sign is reapplied afterward.
        let magnitude = offset * out_span.unsigned_abs();
        let rounded = ((magnitude + in_span / 2) / in_span) as i32;
        let scaled = if out_span < 0 { -rounded } else { rounded };

        (i32::from(start) + scaled) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_are_exact() {
        let map = RangeMap::new(0, 4095, 0, 255);

        assert_eq!(map.map(0), 0);
        assert_eq!(map.map(4095), 255);
    }

    #[test]
    fn endpoints_are_exact_for_non_zero_bounds() {
        let map = RangeMap::new(100, 1100, 10, 200);

        assert_eq!(map.map(100), 10);
        assert_eq!(map.map(1100), 200);
    }

    #[test]
    fn clamps_out_of_bounds_readings() {
        let map = RangeMap::new(100, 1100, 10, 200);

        assert_eq!(map.map(0), 10);
        assert_eq!(map.map(50), 10);
        assert_eq!(map.map(1200), 200);
        assert_eq!(map.map(u16::MAX), 200);
    }

    #[test]
    fn rounds_to_nearest_at_interior_value() {
        // AnalogRange::zero_to(1023).normalize(512) == 32800, i.e. the same
        // widened round-to-nearest rule as here. Pick a case where floor
        // division and round-to-nearest disagree: in_span = 1023,
        // out_span = 255, offset = 512.
        // Exact quotient = 512 * 255 / 1023 = 130560 / 1023 = 127.62...
        // Floor would give 127; round-to-nearest gives 128.
        let map = RangeMap::new(0, 1023, 0, 255);

        assert_eq!(map.map(512), 128);
    }

    #[test]
    fn monotonic_across_a_sweep() {
        let map = RangeMap::new(37, 4013, 3, 251);

        let mut previous = map.map(0);
        for reading in (0..=u16::MAX).step_by(97) {
            let current = map.map(reading);
            assert!(current >= previous, "map regressed at reading {reading}");
            previous = current;
        }
    }

    #[test]
    fn monotonic_decreasing_when_inverted() {
        let map = RangeMap::new(37, 4013, 3, 251).inverted();

        let mut previous = map.map(0);
        for reading in (0..=u16::MAX).step_by(97) {
            let current = map.map(reading);
            assert!(current <= previous, "map increased at reading {reading}");
            previous = current;
        }
    }

    #[test]
    fn inversion_swaps_endpoints() {
        let map = RangeMap::new(0, 4095, 0, 255).inverted();

        assert_eq!(map.map(0), 255);
        assert_eq!(map.map(4095), 0);
    }

    #[test]
    fn double_inversion_restores_original_mapping() {
        let map = RangeMap::new(100, 1100, 10, 200);
        let restored = map.inverted().inverted();

        for reading in [0, 100, 512, 1100, 4095] {
            assert_eq!(restored.map(reading), map.map(reading));
        }
    }

    #[test]
    #[should_panic(expected = "in_min must be less than in_max")]
    fn new_panics_when_in_min_equals_in_max() {
        let _ = RangeMap::new(100, 100, 0, 255);
    }

    #[test]
    fn map_is_total_over_a_full_u16_sweep() {
        let map = RangeMap::new(0, 4095, 0, 255);
        let inverted = RangeMap::new(1, u16::MAX, 5, 250).inverted();

        for reading in (0..=u16::MAX).step_by(31) {
            let _ = map.map(reading);
            let _ = inverted.map(reading);
        }
        // Also cover the exact endpoints.
        let _ = map.map(u16::MAX);
        let _ = inverted.map(u16::MAX);
    }

    #[test]
    fn descending_output_bounds_without_inverted() {
        // `out_min > out_max`: a rising reading lowers the output, no
        // `inverted()` needed (the output span is signed).
        let map = RangeMap::new(0, 100, 255, 0);

        assert_eq!(map.map(0), 255);
        assert_eq!(map.map(100), 0);

        let mut previous = map.map(0);
        for reading in 0..=100 {
            let current = map.map(reading);
            assert!(current <= previous, "map increased at reading {reading}");
            previous = current;
        }
    }

    #[test]
    fn inverted_asymmetric_non_zero_range() {
        // in 100..=1100 (span 1000), out 10..=200, inverted.
        let map = RangeMap::new(100, 1100, 10, 200).inverted();

        assert_eq!(map.map(100), 200); // in_min -> out_max
        assert_eq!(map.map(1100), 10); // in_max -> out_min

        // reading 350 -> offset 250; 250 * 190 / 1000 = 47.5 -> nearest 48.
        // Ascending would give 10 + 48 = 58; inverted gives 200 - 48 = 152.
        assert_eq!(map.map(350), 152);
    }
}
