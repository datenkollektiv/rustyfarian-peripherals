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
}
