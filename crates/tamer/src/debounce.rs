//! Debounced input primitives — [`Debouncer`], [`Edge`], [`EdgeDetector`].
//!
//! The caller owns the clock: pass monotonic `u64` tick values to every
//! `update` call.
//! The tick unit (milliseconds, microseconds, raw timer counts) is your
//! choice; keep it consistent between construction and updates.
//!
//! All elapsed arithmetic uses [`saturating_sub`](u64::saturating_sub) so
//! a non-monotonic timestamp clamps to zero rather than wrapping,
//! which delays a pending transition instead of producing a spurious one.
//!
//! # Example
//!
//! ```
//! use tamer::debounce::{Debouncer, Edge, EdgeDetector};
//!
//! let mut d = Debouncer::new(false, 20);
//! assert_eq!(d.update(true, 0), None);
//! assert_eq!(d.update(true, 25), Some(true));
//!
//! let mut e = EdgeDetector::new(false, 20);
//! assert_eq!(e.update(true, 0), None);
//! assert_eq!(e.update(true, 25), Some(Edge::Rising));
//! ```

/// A time-based boolean signal debouncer.
///
/// Tracks a stable state and requires the raw input to remain at a new level
/// for the full debouncing duration before reporting a transition.
///
/// # Design
///
/// The caller controls the clock by passing monotonic tick values (`u64`).
/// The tick unit is up to the caller — milliseconds, microseconds, or raw
/// timer counts all work, as long as the unit is consistent between the
/// constructor and [`update`](Debouncer::update) calls.
///
/// All internal arithmetic uses [`saturating_sub`](u64::saturating_sub),
/// so a non-monotonic timestamp clamps elapsed time to zero rather than
/// wrapping, delaying the pending transition instead of producing a
/// spurious one.
/// A `u64` tick counter at 1 kHz will not overflow within the lifetime
/// of any real-world deployment.
///
/// # Example
///
/// ```
/// use tamer::debounce::Debouncer;
///
/// let mut debouncer = Debouncer::new(false, 20); // 20-tick debounce window
///
/// // Single changed reading — no transition yet
/// assert_eq!(debouncer.update(true, 0), None);
///
/// // After the debounce period — transition confirmed
/// assert_eq!(debouncer.update(true, 25), Some(true));
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Debouncer {
    stable: bool,
    pending: Option<bool>,
    pending_since: Option<u64>,
    debounce: u64,
}

impl Debouncer {
    /// Creates a new debouncer with the given initial stable state and debounce
    /// duration (in caller-defined ticks).
    ///
    /// A `debounce` of `0` means "no debouncing": the first changed sample
    /// transitions immediately.
    #[must_use]
    pub fn new(initial: bool, debounce: u64) -> Self {
        Self {
            stable: initial,
            pending: None,
            pending_since: None,
            debounce,
        }
    }

    /// Feeds a raw reading at the given timestamp.
    ///
    /// Returns `Some(new_state)` when a stable transition is confirmed.
    /// i.e. the input has been at a different value than the current stable
    /// state for the full debouncing duration.
    ///
    /// Returns `None` if no transition occurred — either the input matches
    /// the stable state, or the debouncing period has not elapsed yet.
    ///
    /// With a debounce window of `0`, the first changed sample transitions
    /// immediately.
    pub fn update(&mut self, raw: bool, now: u64) -> Option<bool> {
        if raw == self.stable {
            // Input matches stable state — cancel any pending transition.
            self.pending = None;
            self.pending_since = None;
            return None;
        }

        // Establish or carry forward the pending value, then evaluate the
        // window in the same call. A freshly started timer has elapsed `0`,
        // so a `0` window confirms immediately while any positive window
        // still requires the value to persist.
        let since = if self.pending == Some(raw) {
            self.pending_since.expect("pending_since set with pending")
        } else {
            self.pending = Some(raw);
            self.pending_since = Some(now);
            now
        };

        if now.saturating_sub(since) >= self.debounce {
            self.stable = raw;
            self.pending = None;
            self.pending_since = None;
            Some(raw)
        } else {
            None
        }
    }

    /// Returns the current stable (debounced) state.
    #[must_use]
    pub fn stable_state(&self) -> bool {
        self.stable
    }
}

/// A detected signal transition direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edge {
    /// The signal transitioned from low to high.
    Rising,
    /// The signal transitioned from high to low.
    Falling,
}

/// Debounced edge detector.
///
/// Wraps a [`Debouncer`] and emits typed [`Edge`] values instead of raw
/// booleans, making the transition direction explicit.
///
/// # Example
///
/// ```
/// use tamer::debounce::{Edge, EdgeDetector};
///
/// let mut edge = EdgeDetector::new(false, 20);
///
/// assert_eq!(edge.update(true, 0), None);
/// assert_eq!(edge.update(true, 25), Some(Edge::Rising));
///
/// assert_eq!(edge.update(false, 50), None);
/// assert_eq!(edge.update(false, 75), Some(Edge::Falling));
/// ```
#[derive(Debug, Clone, Copy)]
pub struct EdgeDetector {
    debouncer: Debouncer,
}

impl EdgeDetector {
    /// Creates a new edge detector with the given initial stable state and
    /// debounce duration (in caller-defined ticks).
    ///
    /// A `debounce` of `0` emits an edge on the first changed sample.
    #[must_use]
    pub fn new(initial: bool, debounce: u64) -> Self {
        Self {
            debouncer: Debouncer::new(initial, debounce),
        }
    }

    /// Feeds a raw reading at the given timestamp.
    ///
    /// Returns `Some(Edge::Rising)` or `Some(Edge::Falling)` when a debounced
    /// transition is confirmed.
    /// Returns `None` otherwise.
    pub fn update(&mut self, raw: bool, now: u64) -> Option<Edge> {
        let previous = self.debouncer.stable_state();
        self.debouncer
            .update(raw, now)
            .map(|new_state| match (previous, new_state) {
                (false, true) => Edge::Rising,
                _ => Edge::Falling,
            })
    }

    /// Returns the current stable (debounced) state.
    #[must_use]
    pub fn stable_state(&self) -> bool {
        self.debouncer.stable_state()
    }
}

/// Thin `embedded-hal` adapter for a debounced digital input pin.
///
/// Reads the pin level on every [`update`](DebouncedInput::update) call,
/// feeds it through an [`EdgeDetector`], and returns the typed edge (if any).
///
/// # Example
///
/// The example uses [`MockInputPin`](crate::mock::MockInputPin) from this
/// crate's own test-support module.
///
/// ```
/// # #[cfg(feature = "hal")] {
/// use tamer::debounce::DebouncedInput;
/// use tamer::mock::MockInputPin;
///
/// let pin = MockInputPin::new(false);
/// let mut input = DebouncedInput::new(pin, false, 20);
///
/// // Pin stays low — no edge.
/// assert_eq!(input.update(0).unwrap(), None);
///
/// // Drive pin high; edge fires after the debounce window.
/// input.pin_mut().set_high();
/// assert_eq!(input.update(1).unwrap(), None);
/// assert_eq!(input.update(25).unwrap(), Some(tamer::debounce::Edge::Rising));
/// # }
/// ```
#[cfg(feature = "hal")]
pub struct DebouncedInput<P> {
    pin: P,
    detector: EdgeDetector,
}

#[cfg(feature = "hal")]
impl<P: embedded_hal::digital::InputPin> DebouncedInput<P> {
    /// Creates a new adapter wrapping `pin` with the given initial logical
    /// level and debounce window (caller-defined ticks).
    ///
    /// `initial` must match the pin's actual level at construction time;
    /// otherwise the first [`update`](DebouncedInput::update) may observe an
    /// artificial transition. Use [`try_from_pin`](DebouncedInput::try_from_pin)
    /// to initialize from the live pin state instead.
    #[must_use]
    pub fn new(pin: P, initial: bool, debounce: u64) -> Self {
        Self {
            pin,
            detector: EdgeDetector::new(initial, debounce),
        }
    }

    /// Creates a new adapter, seeding the initial stable state by reading the
    /// pin once. This avoids the desynchronization risk of passing an explicit
    /// `initial` level to [`new`](DebouncedInput::new).
    ///
    /// # Errors
    ///
    /// Returns `Err(e)` if the initial pin read fails.
    pub fn try_from_pin(mut pin: P, debounce: u64) -> Result<Self, P::Error> {
        let initial = pin.is_high()?;
        Ok(Self {
            pin,
            detector: EdgeDetector::new(initial, debounce),
        })
    }

    /// Reads the pin and ticks the internal [`EdgeDetector`].
    ///
    /// Returns `Ok(Some(edge))` on a confirmed transition,
    /// `Ok(None)` when quiet, or `Err(e)` if the pin read fails.
    pub fn update(&mut self, now: u64) -> Result<Option<Edge>, P::Error> {
        let level = self.pin.is_high()?;
        Ok(self.detector.update(level, now))
    }

    /// Returns the current stable (debounced) state without reading the pin.
    #[must_use]
    pub fn stable_state(&self) -> bool {
        self.detector.stable_state()
    }

    /// Returns a shared reference to the underlying pin.
    pub fn pin(&self) -> &P {
        &self.pin
    }

    /// Returns a mutable reference to the underlying pin.
    ///
    /// Useful in tests to drive the pin level via
    /// [`MockInputPin`](crate::mock::MockInputPin).
    pub fn pin_mut(&mut self) -> &mut P {
        &mut self.pin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Debouncer ---

    #[test]
    fn immediate_change_ignored() {
        let mut d = Debouncer::new(false, 20);
        assert_eq!(d.update(true, 0), None);
        assert!(!d.stable_state());
    }

    #[test]
    fn stable_transition_after_debounce() {
        let mut d = Debouncer::new(false, 20);
        assert_eq!(d.update(true, 0), None);
        assert_eq!(d.update(true, 10), None);
        assert_eq!(d.update(true, 25), Some(true));
        assert!(d.stable_state());
    }

    #[test]
    fn bounce_within_window_resets_timer() {
        let mut d = Debouncer::new(false, 20);
        assert_eq!(d.update(true, 0), None);
        // Bounce back to stable — cancels pending.
        assert_eq!(d.update(false, 10), None);
        // New transition — timer restarts.
        assert_eq!(d.update(true, 15), None);
        // Not enough time from the restart.
        assert_eq!(d.update(true, 30), None);
        // Now enough time from restart at t=15.
        assert_eq!(d.update(true, 40), Some(true));
    }

    #[test]
    fn multiple_transitions() {
        let mut d = Debouncer::new(false, 20);
        // false → true
        assert_eq!(d.update(true, 0), None);
        assert_eq!(d.update(true, 25), Some(true));
        // true → false
        assert_eq!(d.update(false, 50), None);
        assert_eq!(d.update(false, 75), Some(false));
        assert!(!d.stable_state());
    }

    #[test]
    fn transition_at_exact_boundary() {
        let mut d = Debouncer::new(false, 20);
        assert_eq!(d.update(true, 0), None);
        // Exactly at debounce boundary (elapsed == debounce).
        assert_eq!(d.update(true, 20), Some(true));
        assert!(d.stable_state());
    }

    #[test]
    fn same_value_as_stable_clears_pending() {
        let mut d = Debouncer::new(false, 20);
        assert_eq!(d.update(true, 0), None);
        // Return to stable — clears pending.
        assert_eq!(d.update(false, 5), None);
        // Even after the debounce period, no transition.
        assert_eq!(d.update(false, 30), None);
        assert!(!d.stable_state());
    }

    #[test]
    fn zero_debounce_window_transitions_on_first_call() {
        let mut d = Debouncer::new(false, 0);
        // A zero window means no debouncing: the first changed sample confirms.
        assert_eq!(d.update(true, 0), Some(true));
        assert!(d.stable_state());
    }

    #[test]
    fn near_u64_max_ticks_transition_confirmed() {
        let mut d = Debouncer::new(false, 100);
        assert_eq!(d.update(true, u64::MAX - 100), None);
        assert_eq!(d.update(true, u64::MAX - 50), None);
        assert_eq!(d.update(true, u64::MAX), Some(true));
        assert!(d.stable_state());
    }

    #[test]
    fn transition_at_exactly_u64_max() {
        let mut d = Debouncer::new(false, 50);
        assert_eq!(d.update(true, u64::MAX - 50), None);
        assert_eq!(d.update(true, u64::MAX - 1), None);
        assert_eq!(d.update(true, u64::MAX), Some(true));
        assert!(d.stable_state());
    }

    #[test]
    fn saturating_sub_with_earlier_timestamp_does_not_transition() {
        let mut d = Debouncer::new(false, 20);
        assert_eq!(d.update(true, 100), None);
        // Earlier timestamp: elapsed saturates to 0, 0 < 20 → no transition.
        assert_eq!(d.update(true, 50), None);
        assert!(!d.stable_state());
    }

    #[test]
    fn maximum_debounce_window() {
        let mut d = Debouncer::new(false, u64::MAX);
        assert_eq!(d.update(true, 0), None);
        assert_eq!(d.update(true, u64::MAX - 1), None);
        assert_eq!(d.update(true, u64::MAX), Some(true));
        assert!(d.stable_state());
    }

    // --- EdgeDetector ---

    #[test]
    fn rising_edge() {
        let mut e = EdgeDetector::new(false, 20);
        assert_eq!(e.update(true, 0), None);
        assert_eq!(e.update(true, 25), Some(Edge::Rising));
        assert!(e.stable_state());
    }

    #[test]
    fn falling_edge() {
        let mut e = EdgeDetector::new(true, 20);
        assert_eq!(e.update(false, 0), None);
        assert_eq!(e.update(false, 25), Some(Edge::Falling));
        assert!(!e.stable_state());
    }

    #[test]
    fn no_edge_within_debounce_window() {
        let mut e = EdgeDetector::new(false, 20);
        assert_eq!(e.update(true, 0), None);
        assert_eq!(e.update(true, 10), None);
        assert!(!e.stable_state());
    }

    #[test]
    fn edge_bounce_resets_timer() {
        let mut e = EdgeDetector::new(false, 20);
        assert_eq!(e.update(true, 0), None);
        assert_eq!(e.update(false, 10), None);
        assert_eq!(e.update(true, 15), None);
        assert_eq!(e.update(true, 30), None);
        assert_eq!(e.update(true, 40), Some(Edge::Rising));
    }

    #[test]
    fn multiple_edges_in_sequence() {
        let mut e = EdgeDetector::new(false, 20);
        assert_eq!(e.update(true, 0), None);
        assert_eq!(e.update(true, 25), Some(Edge::Rising));
        assert_eq!(e.update(false, 50), None);
        assert_eq!(e.update(false, 75), Some(Edge::Falling));
        assert_eq!(e.update(true, 100), None);
        assert_eq!(e.update(true, 125), Some(Edge::Rising));
        assert!(e.stable_state());
    }

    #[test]
    fn stable_state_delegates_to_debouncer() {
        let e = EdgeDetector::new(true, 20);
        assert!(e.stable_state());
        let e = EdgeDetector::new(false, 20);
        assert!(!e.stable_state());
    }

    #[test]
    fn zero_debounce_rising_edge() {
        let mut e = EdgeDetector::new(false, 0);
        // Zero window: the edge fires on the first changed sample.
        assert_eq!(e.update(true, 0), Some(Edge::Rising));
        assert!(e.stable_state());
    }

    #[test]
    fn zero_debounce_falling_edge() {
        let mut e = EdgeDetector::new(true, 0);
        // Zero window: the edge fires on the first changed sample.
        assert_eq!(e.update(false, 0), Some(Edge::Falling));
        assert!(!e.stable_state());
    }

    #[test]
    fn near_u64_max_ticks_rising_edge() {
        const START: u64 = u64::MAX - 20;
        const DEBOUNCE: u64 = 20;
        let mut e = EdgeDetector::new(false, DEBOUNCE);
        assert_eq!(e.update(true, START), None);
        assert_eq!(e.update(true, START + 19), None);
        assert_eq!(e.update(true, u64::MAX), Some(Edge::Rising));
        assert!(e.stable_state());
    }

    // --- DebouncedInput (hal adapter) ---

    #[cfg(feature = "hal")]
    mod hal_tests {
        use super::super::DebouncedInput;
        use crate::mock::MockInputPin;

        #[test]
        fn debounced_input_rising_edge_via_mock() {
            let pin = MockInputPin::new(false);
            let mut input = DebouncedInput::new(pin, false, 20);

            // Pin low — no edge.
            assert_eq!(input.update(0).unwrap(), None);

            // Drive pin high; debounce not yet elapsed.
            input.pin_mut().set_high();
            assert_eq!(input.update(1).unwrap(), None);
            assert_eq!(input.update(10).unwrap(), None);

            // Past the debouncing window — rising edge fires.
            assert_eq!(input.update(25).unwrap(), Some(super::super::Edge::Rising));
            assert!(input.stable_state());
        }

        #[test]
        fn debounced_input_falling_edge_via_mock() {
            let pin = MockInputPin::new(true);
            let mut input = DebouncedInput::new(pin, true, 20);

            input.pin_mut().set_low();
            assert_eq!(input.update(0).unwrap(), None);
            assert_eq!(input.update(20).unwrap(), Some(super::super::Edge::Falling));
            assert!(!input.stable_state());
        }

        #[test]
        fn debounced_input_pin_accessors() {
            let pin = MockInputPin::new(false);
            let input = DebouncedInput::new(pin, false, 20);
            // Shared ref compiles.
            let _ = input.pin();
        }

        #[test]
        fn debounced_input_bounce_suppressed() {
            let pin = MockInputPin::new(false);
            let mut input = DebouncedInput::new(pin, false, 20);

            // Drive high, then low again (bounce), then high.
            input.pin_mut().set_high();
            assert_eq!(input.update(0).unwrap(), None);
            input.pin_mut().set_low();
            assert_eq!(input.update(10).unwrap(), None);
            input.pin_mut().set_high();
            assert_eq!(input.update(15).unwrap(), None);
            // 15 + 20 = 35: edge fires.
            assert_eq!(input.update(35).unwrap(), Some(super::super::Edge::Rising));
        }

        #[test]
        fn try_from_pin_seeds_initial_state_from_hardware() {
            // Pin is high at construction; try_from_pin reads it…
            let pin = MockInputPin::new(true);
            let mut input = DebouncedInput::try_from_pin(pin, 0).unwrap();
            assert!(input.stable_state());
            // …so a steady-high reading produces no spurious edge.
            assert_eq!(input.update(0).unwrap(), None);
        }

        #[test]
        fn explicit_new_mismatched_initial_state_fires_artificial_edge() {
            // Contract: `new` trusts the caller's `initial`. If it disagrees
            // with the real pin level, the first update sees a transition.
            // Here the pin is high but we claim it started low.
            let pin = MockInputPin::new(true);
            let mut input = DebouncedInput::new(pin, false, 0);
            assert_eq!(input.update(0).unwrap(), Some(super::super::Edge::Rising));
        }
    }
}
