//! Semantic presence detection for digital sensors.
//!
//! A raw GPIO level often means "object present" or "object absent" rather
//! than "button pressed" or "button released".
//! This module names that domain directly with [`Presence`] and [`Polarity`],
//! then composes those with [`Debouncer`](crate::debounce::Debouncer) in
//! [`DigitalPresence`].
//! A `debounce` of `0` means no debouncing: the first changed sample
//! transitions immediately, matching [`Debouncer`](crate::debounce::Debouncer).
//!
//! # Example
//!
//! ```
//! use tamer::presence::{DigitalPresence, Polarity, Presence};
//!
//! // Normally-open reed switch wired to ground with an internal pull-up.
//! // Raw high means absent, raw low means present.
//! let mut reed = DigitalPresence::new(true, Polarity::ActiveLow, 20);
//!
//! assert_eq!(reed.stable_state(), Presence::Absent);
//! assert_eq!(reed.update(false, 0), None);
//! assert_eq!(reed.update(false, 25), Some(Presence::Present));
//! ```

use crate::debounce::Debouncer;

/// Whether a physical object is present or absent.
///
/// Provides a semantic alternative to raw `bool` for binary sensor readings.
/// Use [`Polarity::map`] to convert a raw GPIO level to a `Presence` value.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Presence {
    /// The physical object is detected.
    Present,
    /// The physical object is not detected.
    #[default]
    Absent,
}

impl Presence {
    /// Returns `true` if the state is [`Present`](Presence::Present).
    #[must_use]
    pub const fn is_present(self) -> bool {
        matches!(self, Presence::Present)
    }

    /// Returns `true` if the state is [`Absent`](Presence::Absent).
    #[must_use]
    pub const fn is_absent(self) -> bool {
        matches!(self, Presence::Absent)
    }
}

impl From<bool> for Presence {
    /// Maps `true` to [`Present`](Presence::Present) and `false` to
    /// [`Absent`](Presence::Absent).
    fn from(value: bool) -> Self {
        if value {
            Presence::Present
        } else {
            Presence::Absent
        }
    }
}

impl From<Presence> for bool {
    /// Maps [`Present`](Presence::Present) to `true` and
    /// [`Absent`](Presence::Absent) to `false`.
    fn from(state: Presence) -> Self {
        state.is_present()
    }
}

impl core::ops::Not for Presence {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Presence::Present => Presence::Absent,
            Presence::Absent => Presence::Present,
        }
    }
}

/// How a raw boolean GPIO signal maps to physical presence.
///
/// Different sensor types output opposite logic levels for the same physical
/// state.
/// `Polarity` bridges that gap by mapping raw readings to semantic
/// [`Presence`] values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Polarity {
    /// `true` (high) means present.
    ActiveHigh,
    /// `false` (low) means present.
    ActiveLow,
}

impl Polarity {
    /// Maps a raw boolean reading to a [`Presence`] state.
    #[must_use]
    pub const fn map(self, raw: bool) -> Presence {
        let present = match self {
            Polarity::ActiveHigh => raw,
            Polarity::ActiveLow => !raw,
        };

        if present {
            Presence::Present
        } else {
            Presence::Absent
        }
    }
}

/// Polarity-aware debounced presence detector for digital sensors.
///
/// `DigitalPresence` composes [`Polarity`] and [`Debouncer`] so callers can
/// feed raw GPIO levels and receive semantic debounced
/// [`Presence::Present`] / [`Presence::Absent`] transitions.
///
/// It is intended for binary sensors whose raw level represents physical
/// presence: reed switches, beam breaks, PIR modules, digital Hall switches,
/// capacitive touch modules, and similar inputs.
/// Gesture semantics such as click, double-click, and long-press belong in
/// higher-level button code.
///
/// The caller controls the clock by passing monotonic tick values (`u64`).
/// The tick unit is up to the caller as long as the unit is consistent between
/// construction and [`update`](DigitalPresence::update) calls.
/// Non-monotonic timestamps are tolerated by the underlying [`Debouncer`]
/// enough to avoid spurious transitions, but they are not a supported timing
/// model.
#[derive(Debug, Clone, Copy)]
pub struct DigitalPresence {
    polarity: Polarity,
    debouncer: Debouncer,
}

impl DigitalPresence {
    /// Creates a new digital presence detector.
    ///
    /// `initial_raw` is the initial raw GPIO level.
    /// It is mapped through `polarity` before seeding the debounced stable
    /// state.
    ///
    /// A `debounce` of `0` means no debouncing: the first changed sample
    /// transitions immediately, matching [`Debouncer`] behavior.
    #[must_use]
    pub fn new(initial_raw: bool, polarity: Polarity, debounce: u64) -> Self {
        let initial_presence = polarity.map(initial_raw);
        Self::from_presence(initial_presence, polarity, debounce)
    }

    /// Creates a new detector from an already-semantic initial state.
    ///
    /// This is useful when startup code already knows the semantic state and
    /// does not want to express it as a raw electrical level.
    /// `polarity` is still stored for future raw readings passed to
    /// [`update`](DigitalPresence::update).
    ///
    /// A `debounce` of `0` means no debouncing: the first changed sample
    /// transitions immediately, matching [`Debouncer`] behavior.
    #[must_use]
    pub fn from_presence(initial: Presence, polarity: Polarity, debounce: u64) -> Self {
        Self {
            polarity,
            debouncer: Debouncer::new(initial.is_present(), debounce),
        }
    }

    /// Feeds a raw GPIO reading at the given timestamp.
    ///
    /// Callers should supply monotonic timestamps in a consistent unit.
    /// Non-monotonic timestamps do not produce spurious transitions, but they
    /// are not a supported timing model.
    ///
    /// Returns `Some(Presence::Present)` or `Some(Presence::Absent)` when a
    /// debounced semantic transition is confirmed.
    /// Returns `None` otherwise.
    pub fn update(&mut self, raw: bool, now: u64) -> Option<Presence> {
        let mapped = self.polarity.map(raw);
        self.debouncer
            .update(mapped.is_present(), now)
            .map(Presence::from)
    }

    /// Returns the current stable debounced presence state.
    #[must_use]
    pub fn stable_state(&self) -> Presence {
        Presence::from(self.debouncer.stable_state())
    }

    /// Returns the configured raw-level polarity.
    #[must_use]
    pub const fn polarity(&self) -> Polarity {
        self.polarity
    }
}

/// Thin `embedded-hal` adapter for a digital presence input pin.
///
/// Reads the pin level on every [`update`](DigitalPresenceInput::update) call,
/// maps it through [`Polarity`], and feeds the result through
/// [`DigitalPresence`].
///
/// # Example
///
/// The example uses [`MockInputPin`](crate::mock::MockInputPin) and an
/// active-low reed switch.
///
/// ```
/// # #[cfg(feature = "hal")] {
/// use tamer::mock::MockInputPin;
/// use tamer::presence::{DigitalPresenceInput, Polarity, Presence};
///
/// let pin = MockInputPin::new(true);
/// let mut reed = DigitalPresenceInput::new(pin, true, Polarity::ActiveLow, 20);
///
/// reed.pin_mut().set_low();
/// assert_eq!(reed.update(0).unwrap(), None);
/// assert_eq!(reed.update(20).unwrap(), Some(Presence::Present));
/// # }
/// ```
#[cfg(feature = "hal")]
pub struct DigitalPresenceInput<P> {
    pin: P,
    detector: DigitalPresence,
}

#[cfg(feature = "hal")]
impl<P: embedded_hal::digital::InputPin> DigitalPresenceInput<P> {
    /// Creates a new adapter.
    ///
    /// Use this only when the caller already knows the pin's current electrical
    /// level.
    /// `initial_raw` must match the pin's actual level at construction time;
    /// otherwise the first [`update`](DigitalPresenceInput::update) may observe
    /// an artificial transition.
    /// Prefer [`try_from_pin`](DigitalPresenceInput::try_from_pin) when a live
    /// pin read is available.
    #[must_use]
    pub fn new(pin: P, initial_raw: bool, polarity: Polarity, debounce: u64) -> Self {
        Self {
            pin,
            detector: DigitalPresence::new(initial_raw, polarity, debounce),
        }
    }

    /// Creates a new adapter, seeding the initial raw state by reading the pin
    /// once.
    ///
    /// # Errors
    ///
    /// Returns `Err(e)` if the initial pin read fails.
    pub fn try_from_pin(mut pin: P, polarity: Polarity, debounce: u64) -> Result<Self, P::Error> {
        let initial_raw = pin.is_high()?;
        Ok(Self {
            pin,
            detector: DigitalPresence::new(initial_raw, polarity, debounce),
        })
    }

    /// Reads the pin and ticks the internal [`DigitalPresence`].
    ///
    /// Returns `Ok(Some(presence))` on a confirmed transition,
    /// `Ok(None)` when quiet, or `Err(e)` if the pin read fails.
    pub fn update(&mut self, now: u64) -> Result<Option<Presence>, P::Error> {
        let level = self.pin.is_high()?;
        Ok(self.detector.update(level, now))
    }

    /// Returns the current stable debounced presence state without reading the
    /// pin.
    #[must_use]
    pub fn stable_state(&self) -> Presence {
        self.detector.stable_state()
    }

    /// Returns the configured raw-level polarity.
    #[must_use]
    pub const fn polarity(&self) -> Polarity {
        self.detector.polarity()
    }

    /// Returns a shared reference to the underlying pin.
    pub fn pin(&self) -> &P {
        &self.pin
    }

    /// Returns a mutable reference to the underlying pin.
    ///
    /// Useful in tests to drive the level via
    /// [`MockInputPin`](crate::mock::MockInputPin).
    pub fn pin_mut(&mut self) -> &mut P {
        &mut self.pin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_high_maps_correctly() {
        let polarity = Polarity::ActiveHigh;

        assert_eq!(polarity.map(true), Presence::Present);
        assert_eq!(polarity.map(false), Presence::Absent);
    }

    #[test]
    fn active_low_maps_correctly() {
        let polarity = Polarity::ActiveLow;

        assert_eq!(polarity.map(false), Presence::Present);
        assert_eq!(polarity.map(true), Presence::Absent);
    }

    #[test]
    fn presence_boolean_helpers_match_state() {
        assert!(Presence::Present.is_present());
        assert!(!Presence::Absent.is_present());

        assert!(!Presence::Present.is_absent());
        assert!(Presence::Absent.is_absent());
    }

    #[test]
    fn presence_not_operator_flips_state() {
        assert_eq!(!Presence::Present, Presence::Absent);
        assert_eq!(!Presence::Absent, Presence::Present);
    }

    #[test]
    fn presence_converts_to_and_from_bool() {
        assert_eq!(Presence::from(true), Presence::Present);
        assert_eq!(Presence::from(false), Presence::Absent);

        assert!(bool::from(Presence::Present));
        assert!(!bool::from(Presence::Absent));
    }

    #[test]
    fn active_high_present_transition() {
        let mut sensor = DigitalPresence::new(false, Polarity::ActiveHigh, 20);

        assert_eq!(sensor.stable_state(), Presence::Absent);
        assert_eq!(sensor.update(true, 0), None);
        assert_eq!(sensor.update(true, 10), None);
        assert_eq!(sensor.update(true, 25), Some(Presence::Present));
        assert_eq!(sensor.stable_state(), Presence::Present);
    }

    #[test]
    fn active_low_reed_switch_present_transition() {
        let mut reed = DigitalPresence::new(true, Polarity::ActiveLow, 20);

        assert_eq!(reed.polarity(), Polarity::ActiveLow);
        assert_eq!(reed.stable_state(), Presence::Absent);
        assert_eq!(reed.update(false, 0), None);
        assert_eq!(reed.update(false, 25), Some(Presence::Present));
        assert_eq!(reed.stable_state(), Presence::Present);
    }

    #[test]
    fn from_presence_seeds_semantic_initial_state() {
        let mut sensor =
            DigitalPresence::from_presence(Presence::Present, Polarity::ActiveHigh, 20);

        assert_eq!(sensor.stable_state(), Presence::Present);
        assert_eq!(sensor.polarity(), Polarity::ActiveHigh);
        assert_eq!(sensor.update(false, 0), None);
        assert_eq!(sensor.update(false, 20), Some(Presence::Absent));
    }

    #[test]
    fn absent_transition() {
        let mut reed = DigitalPresence::new(false, Polarity::ActiveLow, 20);

        assert_eq!(reed.stable_state(), Presence::Present);
        assert_eq!(reed.update(true, 100), None);
        assert_eq!(reed.update(true, 125), Some(Presence::Absent));
        assert_eq!(reed.stable_state(), Presence::Absent);
    }

    #[test]
    fn bounce_cancels_pending_transition() {
        let mut reed = DigitalPresence::new(true, Polarity::ActiveLow, 20);

        assert_eq!(reed.update(false, 0), None);
        assert_eq!(reed.update(true, 10), None);
        assert_eq!(reed.update(false, 15), None);
        assert_eq!(reed.update(false, 30), None);
        assert_eq!(reed.update(false, 40), Some(Presence::Present));
    }

    #[test]
    fn zero_debounce_matches_debouncer_behavior() {
        let mut sensor = DigitalPresence::new(false, Polarity::ActiveHigh, 0);

        assert_eq!(sensor.update(true, 0), Some(Presence::Present));
        assert_eq!(sensor.stable_state(), Presence::Present);
    }

    #[test]
    fn non_monotonic_timestamp_does_not_transition_spuriously() {
        let mut sensor = DigitalPresence::new(false, Polarity::ActiveHigh, 20);

        assert_eq!(sensor.update(true, 100), None);
        assert_eq!(sensor.update(true, 50), None);
        assert_eq!(sensor.stable_state(), Presence::Absent);
    }

    #[cfg(feature = "hal")]
    #[test]
    fn hal_adapter_reads_active_low_pin() {
        use crate::mock::MockInputPin;

        let pin = MockInputPin::new(true);
        let mut reed = DigitalPresenceInput::new(pin, true, Polarity::ActiveLow, 20);

        assert_eq!(reed.stable_state(), Presence::Absent);
        reed.pin_mut().set_low();
        assert_eq!(reed.update(0).unwrap(), None);
        assert_eq!(reed.update(20).unwrap(), Some(Presence::Present));
        assert_eq!(reed.stable_state(), Presence::Present);
    }

    #[cfg(feature = "hal")]
    #[test]
    fn hal_adapter_can_seed_from_live_pin() {
        use crate::mock::MockInputPin;

        let pin = MockInputPin::new(false);
        let reed = DigitalPresenceInput::try_from_pin(pin, Polarity::ActiveLow, 20).unwrap();

        assert_eq!(reed.stable_state(), Presence::Present);
        assert_eq!(reed.polarity(), Polarity::ActiveLow);
    }
}
