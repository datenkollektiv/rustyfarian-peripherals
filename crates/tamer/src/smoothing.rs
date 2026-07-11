//! Sliding-window average smoother for noisy ADC readings.
//!
//! [`SlidingAverage`] is a circular buffer that computes the arithmetic mean
//! of the last `N` samples.
//! It is designed to compose with [`crate::hall::HallSensor`] and other
//! threshold-based evaluators: smooth raw ADC readings before evaluation to
//! absorb quantization noise (for example, ESP32-C6 errata ADC-1477).
//!
//! The implementation is zero-alloc and `no_std` compatible — storage is an
//! inline `[u16; N]` array and a running `u32` sum, so `push` is O(1).
//!
//! # Example
//!
//! ```
//! use tamer::smoothing::SlidingAverage;
//!
//! let mut avg = SlidingAverage::<4>::new();
//! assert_eq!(avg.push(100), 100);  // 1 sample: 100/1
//! assert_eq!(avg.push(200), 150);  // 2 samples: 300/2
//! assert_eq!(avg.push(100), 133);  // 3 samples: 400/3 (truncated)
//! assert_eq!(avg.push(200), 150);  // 4 samples: 600/4
//! assert_eq!(avg.push(100), 150);  // full: oldest (100) dropped, 600/4
//! ```

/// Fixed-size sliding window average over `u16` samples.
///
/// Maintains a circular buffer of the last `N` values and returns the
/// arithmetic mean on each [`push`](Self::push).
/// The buffer fills gradually — averages are computed over available samples
/// until full.
///
/// Storage is a plain `[u16; N]` array with a running `u32` sum, so
/// [`push`](Self::push) is **O(1)**: the value being overwritten is
/// subtracted from the sum, the new sample is added, and
/// [`average`](Self::average) returns `sum / count`.
///
/// # Invariant
///
/// `N` must be greater than zero.
/// `SlidingAverage::<0>` is rejected at compile time via an associated
/// `const` assertion.
///
/// # Overflow
///
/// The running sum is `u32`.
/// With `u16` samples, overflow requires `N > u32::MAX / u16::MAX ≈ 65 538`,
/// well beyond any realistic embedded buffer size.
#[derive(Debug, Clone)]
pub struct SlidingAverage<const N: usize> {
    buf: [u16; N],
    head: usize,
    count: usize,
    sum: u32,
}

impl<const N: usize> SlidingAverage<N> {
    /// Compile-time guard: `SlidingAverage::<0>` is forbidden because
    /// `push` would otherwise divide by zero on the modulo step.
    const _N_NONZERO: () = assert!(N > 0, "SlidingAverage requires N > 0");

    /// Creates a new empty sliding average.
    #[must_use]
    pub const fn new() -> Self {
        // Force compile-time evaluation of the N > 0 invariant.
        #[allow(clippy::let_unit_value)]
        let _ = Self::_N_NONZERO;
        Self {
            buf: [0; N],
            head: 0,
            count: 0,
            sum: 0,
        }
    }

    /// Pushes a new sample into the window and returns the updated average.
    ///
    /// The oldest sample is discarded once the buffer is full.
    /// Both the running sum and the count are updated in O(1).
    #[must_use]
    pub fn push(&mut self, sample: u16) -> u16 {
        if self.count == N {
            // Buffer full — subtract the value being overwritten.
            self.sum -= u32::from(self.buf[self.head]);
        } else {
            self.count += 1;
        }
        self.buf[self.head] = sample;
        self.sum += u32::from(sample);
        self.head = (self.head + 1) % N;
        self.average()
    }

    /// Returns the current average without pushing a new sample.
    ///
    /// Returns `0` if no samples have been pushed yet.
    #[must_use]
    pub fn average(&self) -> u16 {
        if self.count == 0 {
            return 0;
        }
        (self.sum / self.count as u32) as u16
    }

    /// Returns the number of samples currently in the window.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.count
    }

    /// Returns `true` if no samples have been pushed.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Returns `true` when the window holds exactly `N` samples.
    #[must_use]
    pub const fn is_full(&self) -> bool {
        self.count == N
    }
}

impl<const N: usize> Default for SlidingAverage<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Exponential moving average (EMA) filter — the float, exponentially-weighted
/// sibling of [`SlidingAverage`].
///
/// The EMA formula is `output = alpha * input + (1 - alpha) * previous_output`.
/// A higher `alpha` tracks the input more closely (less smoothing); a lower
/// `alpha` smooths more aggressively. Unlike [`SlidingAverage`], `EmaFilter`
/// holds no window buffer — a single `f32` accumulator is enough — but its
/// `alpha` weighting is a compile-time tuning constant rather than a window
/// size, so out-of-range values are rejected by [`new`](Self::new) rather
/// than silently clamped.
///
/// The filter is uninitialised until the first sample is provided — the
/// first call to [`update`](Self::update) seeds the filter with the raw
/// value directly, so there is no initial transient.
///
/// # Example
///
/// ```
/// use tamer::smoothing::EmaFilter;
///
/// let mut f = EmaFilter::new(0.1);
/// assert!(f.value().is_none());
///
/// let v = f.update(100.0);
/// assert_eq!(v, 100.0); // first sample initialises directly
///
/// let v = f.update(0.0);
/// assert!(v < 100.0); // subsequent samples blend toward the input
/// ```
#[derive(Debug, Clone, Copy)]
pub struct EmaFilter {
    alpha: f32,
    value: Option<f32>,
}

impl EmaFilter {
    /// Creates a new filter with the given smoothing factor.
    ///
    /// At `alpha = 1.0` the filter passes the input unchanged (no
    /// smoothing). At `alpha` near `0.0` the filter barely moves from its
    /// seeded value.
    ///
    /// # Panics
    ///
    /// Panics if `alpha` is not in `0.0..=1.0`, including `NaN`. `alpha` is a
    /// compile-time-known tuning constant, so an out-of-range value is a
    /// programmer error to surface loudly rather than a runtime condition —
    /// matching the construction-invariant idiom of
    /// [`crate::analog::AnalogRange::new`] and [`crate::range_map::RangeMap::new`].
    #[must_use]
    pub fn new(alpha: f32) -> Self {
        assert!(
            (0.0..=1.0).contains(&alpha),
            "EmaFilter alpha must be in 0.0..=1.0, got {alpha}"
        );
        Self { alpha, value: None }
    }

    /// Feeds one raw sample and returns the filtered value.
    ///
    /// The first call initialises the filter to `raw` exactly, avoiding a
    /// slow convergence from an arbitrary initial value.
    #[must_use]
    pub fn update(&mut self, raw: f32) -> f32 {
        let filtered = match self.value {
            None => raw,
            Some(prev) => self.alpha * raw + (1.0 - self.alpha) * prev,
        };
        self.value = Some(filtered);
        filtered
    }

    /// Returns the current filtered value, or `None` if no sample has been
    /// fed yet.
    #[must_use]
    pub const fn value(&self) -> Option<f32> {
        self.value
    }

    /// Resets the filter to the uninitialised state.
    ///
    /// The next call to [`update`](Self::update) re-initialises from the new
    /// raw value.
    pub fn reset(&mut self) {
        self.value = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_average_is_zero() {
        let avg = SlidingAverage::<4>::new();
        assert_eq!(avg.average(), 0);
        assert!(avg.is_empty());
        assert!(!avg.is_full());
        assert_eq!(avg.len(), 0);
    }

    #[test]
    fn single_sample() {
        let mut avg = SlidingAverage::<4>::new();
        assert_eq!(avg.push(1000), 1000);
        assert_eq!(avg.len(), 1);
        assert!(!avg.is_empty());
        assert!(!avg.is_full());
    }

    #[test]
    fn fills_gradually() {
        let mut avg = SlidingAverage::<4>::new();
        assert_eq!(avg.push(100), 100); // 100/1
        assert_eq!(avg.push(200), 150); // 300/2
        assert_eq!(avg.push(300), 200); // 600/3
        assert_eq!(avg.push(400), 250); // 1000/4
        assert!(avg.is_full());
    }

    #[test]
    fn drops_oldest_when_full() {
        let mut avg = SlidingAverage::<3>::new();
        let _ = avg.push(100);
        let _ = avg.push(200);
        let _ = avg.push(300); // full: [100, 200, 300]
        assert_eq!(avg.average(), 200);

        // Push 400 → drops 100 → [400, 200, 300]
        let result = avg.push(400);
        assert_eq!(result, 300); // (400+200+300)/3
    }

    #[test]
    fn constant_input_converges() {
        let mut avg = SlidingAverage::<8>::new();
        for _ in 0..20 {
            let _ = avg.push(1664);
        }
        assert_eq!(avg.average(), 1664);
    }

    #[test]
    fn spike_absorption() {
        // Simulate ADC-1477 noise: 7 normal readings + 1 spike.
        let mut avg = SlidingAverage::<8>::new();
        for _ in 0..7 {
            let _ = avg.push(1664);
        }
        // +64 spike (ADC-1477 quantization).
        let smoothed = avg.push(1728);
        // Spike dampened: (7*1664 + 1728) / 8 = 1672.
        assert_eq!(smoothed, 1672);
        // Deviation from midpoint: only 8, well below a typical threshold of 30.
        assert!(smoothed.abs_diff(1664) < 30);
    }

    #[test]
    fn integer_truncation() {
        let mut avg = SlidingAverage::<3>::new();
        let _ = avg.push(1);
        let _ = avg.push(1);
        let result = avg.push(2);
        // (1+1+2)/3 = 1.33 → truncated to 1.
        assert_eq!(result, 1);
    }

    #[test]
    fn window_size_one_is_passthrough() {
        let mut avg = SlidingAverage::<1>::new();
        assert_eq!(avg.push(42), 42);
        assert_eq!(avg.push(99), 99);
        assert!(avg.is_full());
    }

    #[test]
    fn max_u16_values_do_not_overflow() {
        // 256 samples of u16::MAX: sum = 256 * 65535 = 16_776_960, fits in u32.
        let mut avg = SlidingAverage::<256>::new();
        for _ in 0..256 {
            let _ = avg.push(u16::MAX);
        }
        assert_eq!(avg.average(), u16::MAX);
    }

    #[test]
    fn default_trait() {
        let avg = SlidingAverage::<4>::default();
        assert!(avg.is_empty());
        assert_eq!(avg.average(), 0);
    }

    #[test]
    fn correct_after_multiple_wraparounds() {
        // Push enough samples to wrap the ring buffer several times and
        // check that `average()` matches a brute-force average over the
        // actual logical window — proving the running sum stays in sync
        // with the buffer contents regardless of `head` position.
        let mut avg = SlidingAverage::<4>::new();
        let stream: [u16; 13] = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130];

        for (i, &s) in stream.iter().enumerate() {
            let result = avg.push(s);

            let window_start = i.saturating_sub(3);
            let window = &stream[window_start..=i];
            let expected: u32 =
                window.iter().map(|&v| u32::from(v)).sum::<u32>() / window.len() as u32;

            assert_eq!(
                result, expected as u16,
                "step {i}: buffer head={}, expected avg of {window:?}",
                avg.head
            );
        }
    }

    #[test]
    fn running_sum_reset_via_zero_samples() {
        // Pushing zeros after non-zero samples should still produce the
        // correct running mean (regression test for accidental sum drift).
        let mut avg = SlidingAverage::<3>::new();
        let _ = avg.push(300);
        let _ = avg.push(300);
        let _ = avg.push(300);
        // Now drop all 300s and replace with zeros.
        assert_eq!(avg.push(0), 200); // (300 + 300 + 0) / 3
        assert_eq!(avg.push(0), 100); // (300 +   0 + 0) / 3
        assert_eq!(avg.push(0), 0); //   (0 +   0 + 0) / 3
    }

    // ── EmaFilter ───────────────────────────────────────────────────────

    fn assert_approx(actual: f32, expected: f32, tolerance: f32) {
        let diff = (actual - expected).abs();
        assert!(
            diff <= tolerance,
            "expected {expected} +/- {tolerance}, got {actual} (diff {diff})"
        );
    }

    #[test]
    fn ema_value_none_before_first_sample() {
        let f = EmaFilter::new(0.5);
        assert!(f.value().is_none());
    }

    #[test]
    fn ema_first_sample_initialises_to_raw_value() {
        let mut f = EmaFilter::new(0.5);
        let v = f.update(42.0);
        assert_eq!(v, 42.0);
        assert_eq!(f.value(), Some(42.0));
    }

    #[test]
    fn ema_constant_input_converges() {
        let mut f = EmaFilter::new(0.3);
        let _ = f.update(100.0);
        for _ in 0..50 {
            let _ = f.update(100.0);
        }
        // After many identical samples the output should be extremely close to 100.
        assert_approx(f.value().unwrap(), 100.0, 0.01);
    }

    #[test]
    fn ema_alpha_one_passes_input_unchanged() {
        let mut f = EmaFilter::new(1.0);
        let _ = f.update(10.0);
        let v = f.update(99.0);
        assert_eq!(v, 99.0);
    }

    #[test]
    fn ema_alpha_zero_freezes_after_first_sample() {
        let mut f = EmaFilter::new(0.0);
        let first = f.update(1.0);
        assert_eq!(first, 1.0); // first sample always seeds directly
        let second = f.update(1000.0);
        // alpha=0 means the input never moves the filter after seeding.
        assert_eq!(second, 1.0);
    }

    #[test]
    fn ema_reset_clears_state() {
        let mut f = EmaFilter::new(0.5);
        let _ = f.update(100.0);
        assert!(f.value().is_some());
        f.reset();
        assert!(f.value().is_none());
        // After reset the next update re-initialises.
        let v = f.update(7.0);
        assert_eq!(v, 7.0);
    }

    #[test]
    #[should_panic(expected = "EmaFilter alpha must be in 0.0..=1.0")]
    fn ema_new_panics_on_alpha_below_range() {
        let _ = EmaFilter::new(-0.01);
    }

    #[test]
    #[should_panic(expected = "EmaFilter alpha must be in 0.0..=1.0")]
    fn ema_new_panics_on_alpha_above_range() {
        let _ = EmaFilter::new(1.01);
    }

    #[test]
    #[should_panic(expected = "EmaFilter alpha must be in 0.0..=1.0")]
    fn ema_new_panics_on_nan_alpha() {
        let _ = EmaFilter::new(f32::NAN);
    }
}
