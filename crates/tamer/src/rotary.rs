//! Quadrature rotary encoder decoder — [`QuadratureDecoder`] and
//! [`EncoderDirection`].
//!
//! # Decoding
//!
//! A rotary encoder outputs two square waves (A and B) 90° out of phase.
//! Together they form a 2-bit Gray-code sequence.
//! For a typical EC11 encoder rotating clockwise, the sequence is:
//!
//! ```text
//! 11 → 01 → 00 → 10 → 11   (CW)
//! 11 → 10 → 00 → 01 → 11   (CCW)
//! ```
//!
//! # Accumulator debouncing
//!
//! Contact bounce causes rapid, spurious transitions between adjacent states.
//! Rather than emitting an event on every valid transition, the decoder
//! accumulates half-steps and only emits a position change when the
//! accumulator reaches `±steps_per_detent`.
//! Spurious transitions in the opposite direction subtract from the
//! accumulator, naturally cancelling noise without time-based logic.
//!
//! # Skipped samples
//!
//! A sample where A and B both changed at once (a 2-bit jump) carries no
//! direction, so [`QuadratureDecoder::update`] drops it.
//! The decoder still counts it in
//! [`skipped_count`](QuadratureDecoder::skipped_count), so a consumer whose
//! sampling is starved can detect that intermediate states were missed.
//!
//! # Example
//!
//! ```
//! use tamer::rotary::{EncoderDirection, QuadratureDecoder};
//!
//! let mut dec = QuadratureDecoder::new(true, true, 4);
//!
//! // CW: 11 → 01 → 00 → 10 → 11
//! dec.update(false, true);
//! dec.update(false, false);
//! dec.update(true, false);
//! assert_eq!(dec.update(true, true), Some(EncoderDirection::Clockwise));
//! assert_eq!(dec.position(), 1);
//! ```

use crate::button::ButtonEvent;

/// The direction of a confirmed encoder detent step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncoderDirection {
    /// The encoder moved clockwise (position increased by 1).
    Clockwise,
    /// The encoder moved counter-clockwise (position decreased by 1).
    CounterClockwise,
}

/// Synchronous core of a rotary encoder with a push button.
///
/// This is the tier-agnostic contract shared by the ESP-IDF
/// `rustyfarian_esp_idf_peripherals::rotary::Encoder` and the esp-hal
/// `rustyfarian_esp_hal_peripherals::rotary::InterruptEncoder`.
/// Application code written against `E: RotaryEncoder` runs unchanged on either tier and against [`NoopRotaryEncoder`] on the host.
///
/// # Contract
///
/// - [`update`](Self::update) polls only the button and returns a completed gesture, if any.
///   Gestures are measured against `now_ms`, so pass the same monotonic millisecond clock on every call.
/// - [`position`](Self::position) reflects the detents decoded so far, including those that arrived while the caller was blocked.
///   Implementors decode rotation independently of the caller's polling loop (in the tiers: a GPIO interrupt).
///   Capture is still bounded by interrupt latency and by edges the implementation cannot observe.
/// - The position wraps on `i32` overflow and never panics, so callers can take deltas with `wrapping_sub`.
/// - [`set_position`](Self::set_position) overwrites the position and discards any partial detent.
/// - [`reset`](Self::reset) is `set_position(0)`.
/// - [`is_button_pressed`](Self::is_button_pressed) returns the debounced button state.
///
/// # Thread safety
///
/// The trait has no `Send` or `Sync` bound.
/// Some implementors (the esp-hal `InterruptEncoder`) are `!Send`, so generic code must not require `E: Send`.
///
/// # Not implemented by the async esp-hal `Encoder`
///
/// `rustyfarian_esp_hal_peripherals::rotary::Encoder` deliberately does **not** implement this trait.
/// Its `position()` only advances while `wait_for_step` is being polled, so behind this trait it would look like a drop-in but lose detents whenever the caller blocks.
/// Use `InterruptEncoder` on esp-hal when you need this trait.
///
/// # Out of scope
///
/// Diagnostics (`isr_count`, `edge_count`, `skipped_count`, `pin_a_is_high`, `pin_b_is_high`) and tier-specific extras (`wait_for_step`, constructors, config) stay as inherent methods on the concrete types.
/// Constructors are not part of the trait because they take tier-specific pin and peripheral types.
///
/// # Evolution
///
/// The trait is not sealed, so downstream crates may implement it.
/// New methods will only be added with default bodies.
///
/// # Examples
///
/// ```
/// use tamer::button::ButtonEvent;
/// use tamer::rotary::{NoopRotaryEncoder, RotaryEncoder};
///
/// /// Tier-agnostic knob tick: one function for ESP-IDF, esp-hal, and host tests.
/// fn on_tick<E: RotaryEncoder>(enc: &mut E, now_ms: u64) -> (i32, Option<ButtonEvent>) {
///     let event = enc.update(now_ms);
///     if event == Some(ButtonEvent::LongPress) {
///         enc.reset();
///     }
///     (enc.position(), event)
/// }
///
/// let mut enc = NoopRotaryEncoder::new();
/// enc.set_position(7);
/// assert_eq!(on_tick(&mut enc, 0), (7, None));
/// assert!(!enc.is_button_pressed());
/// ```
pub trait RotaryEncoder {
    /// Polls the button and returns a debounced press/release/gesture event, if one completed.
    ///
    /// `now_ms` is a monotonic millisecond timestamp.
    /// Call this regularly; a gesture is only ever reported on a call to `update`.
    fn update(&mut self, now_ms: u64) -> Option<ButtonEvent>;

    /// Returns the current encoder position in detents.
    ///
    /// Takes `&self` and never blocks.
    /// Overflow wraps (`i32::MAX` + 1 detent is `i32::MIN`) and must not panic.
    #[must_use]
    fn position(&self) -> i32;

    /// Overwrites the position and clears the decoder's partial-detent accumulator.
    ///
    /// Any `i32` is valid, including the extremes; later detents wrap rather than panic.
    fn set_position(&mut self, position: i32);

    /// Resets the position to zero.
    ///
    /// Equivalent to `set_position(0)`.
    fn reset(&mut self) {
        self.set_position(0);
    }

    /// Returns the current debounced (stable) button-pressed state.
    #[must_use]
    fn is_button_pressed(&self) -> bool;
}

/// Do-nothing [`RotaryEncoder`] for host tests and trait-generic code.
///
/// It never produces a button event and its button is never pressed.
/// It does honour the position contract so generic code can be exercised:
/// [`set_position`](RotaryEncoder::set_position) and [`reset`](RotaryEncoder::reset) are observable through [`position`](RotaryEncoder::position).
/// It does not decode rotation (there is no input to decode), so the position only changes through those two calls.
///
/// [`Default`] yields position `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NoopRotaryEncoder {
    position: i32,
}

impl NoopRotaryEncoder {
    /// Creates a mock encoder at position `0`.
    #[must_use]
    pub const fn new() -> Self {
        Self { position: 0 }
    }
}

impl RotaryEncoder for NoopRotaryEncoder {
    fn update(&mut self, _now_ms: u64) -> Option<ButtonEvent> {
        None
    }

    fn position(&self) -> i32 {
        self.position
    }

    fn set_position(&mut self, position: i32) {
        self.position = position;
    }

    fn is_button_pressed(&self) -> bool {
        false
    }
}

/// Quadrature transition lookup table.
///
/// Index: `(prev_state << 2) | curr_state` where `state = (A as u8) << 1 | B as u8`.
///
/// Values: `+1` = one CW half-step, `-1` = one CCW half-step, `0` = no change
/// (same state) or invalid transition (2-bit jump, treated as bounce/glitch).
static QUAD_TABLE: [i8; 16] = [
    0,  // 00→00: no change
    -1, // 00→01: CCW
    1,  // 00→10: CW
    0,  // 00→11: invalid (2-bit jump)
    1,  // 01→00: CW
    0,  // 01→01: no change
    0,  // 01→10: invalid (2-bit jump)
    -1, // 01→11: CCW
    -1, // 10→00: CCW
    0,  // 10→01: invalid (2-bit jump)
    0,  // 10→10: no change
    1,  // 10→11: CW
    0,  // 11→00: invalid (2-bit jump)
    1,  // 11→01: CW
    -1, // 11→10: CCW
    0,  // 11→11: no change
];

/// Pure quadrature decoder with accumulator-based debouncing.
///
/// Feed GPIO samples via [`update`](QuadratureDecoder::update) and read the
/// accumulated [`position`](QuadratureDecoder::position).
///
/// # Example
///
/// ```
/// use tamer::rotary::{EncoderDirection, QuadratureDecoder};
///
/// let mut dec = QuadratureDecoder::new(true, true, 4);
///
/// // One CCW detent: 11 → 10 → 00 → 01 → 11
/// dec.update(true, false);
/// dec.update(false, false);
/// dec.update(false, true);
/// assert_eq!(dec.update(true, true), Some(EncoderDirection::CounterClockwise));
/// assert_eq!(dec.position(), -1);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct QuadratureDecoder {
    last_state: u8,
    accumulator: i32,
    steps_per_detent: i32,
    position: i32,
    skipped: u32,
}

impl QuadratureDecoder {
    /// Creates a new decoder.
    ///
    /// `initial_a` / `initial_b`: current GPIO levels at construction time.
    ///
    /// `steps_per_detent`: valid quadrature transitions per physical detent.
    /// Use `4` for full-step EC11 encoders (most common), `2` for half-step.
    /// Must be greater than zero.
    ///
    /// # Panics
    ///
    /// Panics if `steps_per_detent` is `0`.
    #[must_use]
    pub fn new(initial_a: bool, initial_b: bool, steps_per_detent: u8) -> Self {
        assert!(
            steps_per_detent > 0,
            "steps_per_detent must be greater than zero, got {steps_per_detent}"
        );
        Self {
            last_state: (initial_a as u8) << 1 | initial_b as u8,
            accumulator: 0,
            steps_per_detent: i32::from(steps_per_detent),
            position: 0,
            skipped: 0,
        }
    }

    /// Feeds a new A/B sample.
    ///
    /// Returns `Some(EncoderDirection::Clockwise)` or
    /// `Some(EncoderDirection::CounterClockwise)` when the accumulator
    /// reaches a full detent.
    /// Returns `None` for partial steps, bounce, or invalid transitions.
    /// An invalid transition (both pins changed) is also counted in
    /// [`skipped_count`](QuadratureDecoder::skipped_count).
    pub fn update(&mut self, a: bool, b: bool) -> Option<EncoderDirection> {
        let state = (a as u8) << 1 | b as u8;
        self.skipped = self
            .skipped
            .wrapping_add(u32::from((self.last_state ^ state) == 0b11));
        let step = QUAD_TABLE[(self.last_state << 2 | state) as usize];
        self.last_state = state;
        self.accumulator += i32::from(step);

        if self.accumulator >= self.steps_per_detent {
            self.position = self.position.wrapping_add(1);
            self.accumulator = 0;
            Some(EncoderDirection::Clockwise)
        } else if self.accumulator <= -self.steps_per_detent {
            self.position = self.position.wrapping_sub(1);
            self.accumulator = 0;
            Some(EncoderDirection::CounterClockwise)
        } else {
            None
        }
    }

    /// Returns the absolute position (increases CW, decreases CCW).
    ///
    /// The position wraps on `i32` overflow (`i32::MAX` + 1 CW detent is `i32::MIN`, and vice versa) and never panics.
    /// Use `wrapping_sub` to compute deltas across the boundary.
    #[must_use]
    pub fn position(&self) -> i32 {
        self.position
    }

    /// Returns how many samples skipped a Gray-code state since construction.
    ///
    /// A skipped sample is one where A and B changed at once (`00↔11` or
    /// `01↔10`): at least one intermediate state was missed, or the contacts
    /// bounced.
    /// [`update`](QuadratureDecoder::update) drops such a sample, so a
    /// growing count signals that sampling is too slow and detents may be
    /// lost — it does not prove a detent was lost.
    /// A zero count does not prove nothing was lost either: missing exactly
    /// two states lands on a valid neighbour, and a missed full cycle looks
    /// like no change.
    ///
    /// The count is monotonic, wraps at `u32::MAX`, and is not cleared by
    /// [`set_position`](QuadratureDecoder::set_position) or
    /// [`reset`](QuadratureDecoder::reset).
    /// Measure a window by taking deltas with `wrapping_sub`.
    ///
    /// # Example
    ///
    /// ```
    /// use tamer::rotary::QuadratureDecoder;
    ///
    /// let mut dec = QuadratureDecoder::new(true, true, 4);
    /// let before = dec.skipped_count();
    ///
    /// // CW 11 → 01, then the 00 sample is missed: 01 → 10 is a 2-bit jump.
    /// dec.update(false, true);
    /// assert_eq!(dec.update(true, false), None);
    /// assert_eq!(dec.skipped_count().wrapping_sub(before), 1);
    /// ```
    #[must_use]
    pub fn skipped_count(&self) -> u32 {
        self.skipped
    }

    /// Overwrites the absolute position and clears the accumulator.
    ///
    /// The [`skipped_count`](QuadratureDecoder::skipped_count) is preserved.
    pub fn set_position(&mut self, position: i32) {
        self.position = position;
        self.accumulator = 0;
    }

    /// Resets the position to zero and clears the accumulator.
    ///
    /// The [`skipped_count`](QuadratureDecoder::skipped_count) is preserved.
    pub fn reset(&mut self) {
        self.position = 0;
        self.accumulator = 0;
    }
}

/// Thin `embedded-hal` adapter that drives a [`QuadratureDecoder`] from two
/// digital input pins.
///
/// Both pins must share the same `Error` type.
/// If your HAL uses different error types for each pin, newtype-wrap the second
/// pin to unify them, or use a HAL (such as `esp-idf-hal`) that already uses a
/// single error type for all GPIO.
///
/// # Example
///
/// The example uses [`MockInputPin`](crate::mock::MockInputPin) for both
/// channels.
///
/// ```
/// # #[cfg(feature = "hal")] {
/// use tamer::rotary::{EncoderDirection, QuadratureInput};
/// use tamer::mock::MockInputPin;
///
/// let pin_a = MockInputPin::new(true);
/// let pin_b = MockInputPin::new(true);
/// let mut enc = QuadratureInput::new(pin_a, pin_b, true, true, 4);
///
/// // Drive CW: 11 → 01
/// enc.pin_a_mut().set_low();
/// assert_eq!(enc.update().unwrap(), None);
/// # }
/// ```
#[cfg(feature = "hal")]
pub struct QuadratureInput<A, B> {
    pin_a: A,
    pin_b: B,
    decoder: QuadratureDecoder,
}

#[cfg(feature = "hal")]
impl<A, B> QuadratureInput<A, B>
where
    A: embedded_hal::digital::InputPin,
    B: embedded_hal::digital::InputPin<Error = A::Error>,
{
    /// Creates a new adapter.
    ///
    /// `pin_a` / `pin_b`: the two quadrature input pins.
    ///
    /// `initial_a` / `initial_b`: the logical levels at construction time.
    /// These must match the pins' actual levels; otherwise the first
    /// [`update`](QuadratureInput::update) may register an artificial
    /// transition. Use [`try_from_pins`](QuadratureInput::try_from_pins) to
    /// seed from the live pin state instead.
    ///
    /// `steps_per_detent`: passed through to [`QuadratureDecoder::new`].
    ///
    /// # Panics
    ///
    /// Panics if `steps_per_detent` is `0`.
    /// See [`QuadratureDecoder::new`] for the invariant.
    #[must_use]
    pub fn new(pin_a: A, pin_b: B, initial_a: bool, initial_b: bool, steps_per_detent: u8) -> Self {
        Self {
            pin_a,
            pin_b,
            decoder: QuadratureDecoder::new(initial_a, initial_b, steps_per_detent),
        }
    }

    /// Creates a new adapter, seeding the decoder's initial state by reading
    /// both pins once. This avoids the desynchronization risk of passing
    /// explicit `initial_a` / `initial_b` levels to [`new`](QuadratureInput::new).
    ///
    /// # Errors
    ///
    /// Returns `Err(e)` if either initial pin read fails.
    ///
    /// # Panics
    ///
    /// Panics if `steps_per_detent` is `0`.
    pub fn try_from_pins(
        mut pin_a: A,
        mut pin_b: B,
        steps_per_detent: u8,
    ) -> Result<Self, A::Error> {
        let initial_a = pin_a.is_high()?;
        let initial_b = pin_b.is_high()?;
        Ok(Self {
            pin_a,
            pin_b,
            decoder: QuadratureDecoder::new(initial_a, initial_b, steps_per_detent),
        })
    }

    /// Reads both pins and ticks the internal [`QuadratureDecoder`].
    ///
    /// Returns `Ok(Some(direction))` on a confirmed detent step,
    /// `Ok(None)` for partial/noisy transitions,
    /// or `Err(e)` if either pin read fails.
    pub fn update(&mut self) -> Result<Option<EncoderDirection>, A::Error> {
        let a = self.pin_a.is_high()?;
        let b = self.pin_b.is_high()?;
        Ok(self.decoder.update(a, b))
    }

    /// Returns the accumulated position from the underlying decoder.
    #[must_use]
    pub fn position(&self) -> i32 {
        self.decoder.position()
    }

    /// Returns the skipped-sample count from the underlying decoder.
    ///
    /// See [`QuadratureDecoder::skipped_count`].
    #[must_use]
    pub fn skipped_count(&self) -> u32 {
        self.decoder.skipped_count()
    }

    /// Overwrites the position and clears the accumulator.
    pub fn set_position(&mut self, position: i32) {
        self.decoder.set_position(position);
    }

    /// Resets position to zero and clears the accumulator.
    pub fn reset(&mut self) {
        self.decoder.reset();
    }

    /// Returns a shared reference to pin A.
    pub fn pin_a(&self) -> &A {
        &self.pin_a
    }

    /// Returns a shared reference to pin B.
    pub fn pin_b(&self) -> &B {
        &self.pin_b
    }

    /// Returns a mutable reference to pin A.
    pub fn pin_a_mut(&mut self) -> &mut A {
        &mut self.pin_a
    }

    /// Returns a mutable reference to pin B.
    pub fn pin_b_mut(&mut self) -> &mut B {
        &mut self.pin_b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoder() -> QuadratureDecoder {
        // Start at state 11 (both pins high — typical resting detent position).
        QuadratureDecoder::new(true, true, 4)
    }

    /// Drive one full CW quadrature cycle: 11 → 01 → 00 → 10 → 11.
    fn one_cw(dec: &mut QuadratureDecoder) -> Option<EncoderDirection> {
        dec.update(false, true); // 11→01
        dec.update(false, false); // 01→00
        dec.update(true, false); // 00→10
        dec.update(true, true) // 10→11 — returns Some(Clockwise) here
    }

    /// Drive one full CCW quadrature cycle: 11 → 10 → 00 → 01 → 11.
    fn one_ccw(dec: &mut QuadratureDecoder) -> Option<EncoderDirection> {
        dec.update(true, false); // 11→10
        dec.update(false, false); // 10→00
        dec.update(false, true); // 00→01
        dec.update(true, true) // 01→11 — returns Some(CounterClockwise) here
    }

    #[test]
    fn cw_one_detent_increments_position() {
        let mut dec = decoder();
        assert_eq!(one_cw(&mut dec), Some(EncoderDirection::Clockwise));
        assert_eq!(dec.position(), 1);
    }

    #[test]
    fn ccw_one_detent_decrements_position() {
        let mut dec = decoder();
        assert_eq!(one_ccw(&mut dec), Some(EncoderDirection::CounterClockwise));
        assert_eq!(dec.position(), -1);
    }

    #[test]
    fn multiple_cw_detents_accumulate() {
        let mut dec = decoder();
        for i in 1..=5_i32 {
            assert_eq!(one_cw(&mut dec), Some(EncoderDirection::Clockwise));
            assert_eq!(dec.position(), i);
        }
    }

    #[test]
    fn multiple_ccw_detents_accumulate() {
        let mut dec = decoder();
        for i in 1..=5_i32 {
            assert_eq!(one_ccw(&mut dec), Some(EncoderDirection::CounterClockwise));
            assert_eq!(dec.position(), -i);
        }
    }

    #[test]
    fn cw_then_ccw_returns_to_zero() {
        let mut dec = decoder();
        one_cw(&mut dec);
        one_ccw(&mut dec);
        assert_eq!(dec.position(), 0);
    }

    #[test]
    fn invalid_two_bit_jump_is_ignored() {
        let mut dec = decoder();
        // 11 → 00 is a 2-bit jump; must not count.
        assert_eq!(dec.update(false, false), None);
        assert_eq!(dec.position(), 0);
        // 00 → 11 is also a 2-bit jump.
        assert_eq!(dec.update(true, true), None);
        assert_eq!(dec.position(), 0);
    }

    #[test]
    fn bounce_at_detent_does_not_drift() {
        let mut dec = decoder();
        // Rapid oscillation between 11 and 10 without completing a cycle.
        for _ in 0..20 {
            dec.update(true, false); // 11→10: acc = -1
            dec.update(true, true); // 10→11: acc = 0
        }
        assert_eq!(dec.position(), 0);
    }

    #[test]
    fn partial_rotation_reversed_before_detent_does_not_count() {
        let mut dec = decoder();
        // 3 steps CW (acc = 3, not yet a full detent)…
        dec.update(false, true); // 11→01: acc = 1
        dec.update(false, false); // 01→00: acc = 2
        dec.update(true, false); // 00→10: acc = 3
                                 // …then 3 steps CCW back.
        dec.update(false, false); // 10→00: acc = 2
        dec.update(false, true); // 00→01: acc = 1
        dec.update(true, true); // 01→11: acc = 0
        assert_eq!(dec.position(), 0);
    }

    #[test]
    fn set_position_mid_detent_discards_partial_cw_progress() {
        let mut dec = decoder();
        dec.update(false, true); // 11→01
        dec.update(false, false); // 01→00
        dec.update(true, false); // 00→10 — 3 of 4 transitions
        dec.set_position(42);
        assert_eq!(dec.position(), 42);
        // The 4th transition alone must not complete a detent.
        assert_eq!(dec.update(true, true), None); // 10→11
        assert_eq!(dec.position(), 42);
    }

    #[test]
    fn set_position_mid_detent_discards_partial_ccw_progress() {
        let mut dec = decoder();
        dec.update(true, false); // 11→10
        dec.update(false, false); // 10→00
        dec.update(false, true); // 00→01 — 3 of 4 transitions
        dec.set_position(42);
        assert_eq!(dec.position(), 42);
        assert_eq!(dec.update(true, true), None); // 01→11
        assert_eq!(dec.position(), 42);
    }

    #[test]
    fn reset_mid_detent_discards_partial_cw_progress() {
        let mut dec = decoder();
        one_cw(&mut dec);
        dec.update(false, true); // 11→01
        dec.update(false, false); // 01→00
        dec.update(true, false); // 00→10 — 3 of 4 transitions
        dec.reset();
        assert_eq!(dec.position(), 0);
        assert_eq!(dec.update(true, true), None); // 10→11
        assert_eq!(dec.position(), 0);
    }

    #[test]
    fn reset_mid_detent_discards_partial_ccw_progress() {
        let mut dec = decoder();
        one_ccw(&mut dec);
        dec.update(true, false); // 11→10
        dec.update(false, false); // 10→00
        dec.update(false, true); // 00→01 — 3 of 4 transitions
        dec.reset();
        assert_eq!(dec.position(), 0);
        assert_eq!(dec.update(true, true), None); // 01→11
        assert_eq!(dec.position(), 0);
    }

    #[test]
    fn position_wraps_from_max_to_min_on_cw_detent() {
        let mut dec = decoder();
        dec.set_position(i32::MAX);
        assert_eq!(one_cw(&mut dec), Some(EncoderDirection::Clockwise));
        assert_eq!(dec.position(), i32::MIN);
    }

    #[test]
    fn position_wraps_from_min_to_max_on_ccw_detent() {
        let mut dec = decoder();
        dec.set_position(i32::MIN);
        assert_eq!(one_ccw(&mut dec), Some(EncoderDirection::CounterClockwise));
        assert_eq!(dec.position(), i32::MAX);
    }

    #[test]
    fn half_step_config_counts_every_two_transitions() {
        let mut dec = QuadratureDecoder::new(true, true, 2);
        // With steps_per_detent=2, a half cycle should register.
        assert_eq!(dec.update(false, true), None); // 11→01: acc=1
        assert_eq!(dec.position(), 0);
        assert_eq!(dec.update(false, false), Some(EncoderDirection::Clockwise)); // 01→00: acc=2 → fires
        assert_eq!(dec.position(), 1);
    }

    #[test]
    #[should_panic(expected = "steps_per_detent must be greater than zero")]
    fn zero_steps_per_detent_panics() {
        let _ = QuadratureDecoder::new(false, false, 0);
    }

    #[test]
    fn max_steps_per_detent_accepted() {
        // The `i32` accumulator makes any non-zero `u8` valid — no upper bound.
        let mut dec = QuadratureDecoder::new(true, true, 255);
        // A single half-step never reaches a 255-step detent.
        assert_eq!(dec.update(false, true), None);
        assert_eq!(dec.position(), 0);
    }

    #[test]
    fn sustained_rotation_never_overflows_accumulator() {
        // Degenerate/long-run input: keep turning one way for far more detents
        // than any real session. The `i32` accumulator resets to 0 every detent,
        // so it stays bounded by `steps_per_detent` (4) forever — it never
        // approaches any integer limit and never panics in a debug build. This
        // pins the no-overflow invariant so a future change to the accumulator
        // type or the reset-on-detent logic can't silently reintroduce the
        // former `i8`-era overflow bug.
        let mut dec = decoder();
        for i in 1..=100_000_i32 {
            assert_eq!(one_cw(&mut dec), Some(EncoderDirection::Clockwise));
            assert_eq!(dec.position(), i);
        }
    }

    #[test]
    fn sustained_reverse_rotation_never_overflows_accumulator() {
        // Symmetric counterpart to `sustained_rotation_never_overflows_accumulator`:
        // the same bounded-accumulator invariant must hold turning the other way,
        // so the no-overflow guarantee is pinned for both directions.
        let mut dec = decoder();
        for i in 1..=100_000_i32 {
            assert_eq!(one_ccw(&mut dec), Some(EncoderDirection::CounterClockwise));
            assert_eq!(dec.position(), -i);
        }
    }

    // --- skipped_count ---

    #[test]
    fn clean_cw_cycles_do_not_count_skips() {
        let mut dec = decoder();
        for _ in 0..3 {
            one_cw(&mut dec);
        }
        assert_eq!(dec.skipped_count(), 0);
    }

    #[test]
    fn clean_ccw_cycles_do_not_count_skips() {
        let mut dec = decoder();
        for _ in 0..3 {
            one_ccw(&mut dec);
        }
        assert_eq!(dec.skipped_count(), 0);
    }

    #[test]
    fn cw_cycle_with_missed_sample_counts_one_skip() {
        let mut dec = decoder();
        assert_eq!(dec.update(false, true), None); // 11→01
        assert_eq!(dec.update(true, false), None); // 01→10: 00 missed
        assert_eq!(dec.update(true, true), None); // 10→11: detent not reached
        assert_eq!(dec.skipped_count(), 1);
        assert_eq!(dec.position(), 0);
    }

    #[test]
    fn ccw_cycle_with_missed_sample_counts_one_skip() {
        let mut dec = decoder();
        assert_eq!(dec.update(true, false), None); // 11→10
        assert_eq!(dec.update(false, true), None); // 10→01: 00 missed
        assert_eq!(dec.update(true, true), None); // 01→11: detent not reached
        assert_eq!(dec.skipped_count(), 1);
        assert_eq!(dec.position(), 0);
    }

    #[test]
    fn every_two_bit_jump_counts_once() {
        // 00→11, 11→00, 01→10, 10→01
        let jumps = [
            ((false, false), (true, true)),
            ((true, true), (false, false)),
            ((false, true), (true, false)),
            ((true, false), (false, true)),
        ];
        for ((a0, b0), (a1, b1)) in jumps {
            let mut dec = QuadratureDecoder::new(a0, b0, 4);
            assert_eq!(dec.update(a1, b1), None);
            assert_eq!(dec.skipped_count(), 1);
        }
    }

    #[test]
    fn repeated_same_state_does_not_count_skips() {
        let mut dec = decoder();
        for _ in 0..10 {
            dec.update(true, true);
        }
        assert_eq!(dec.skipped_count(), 0);
    }

    #[test]
    fn reset_and_set_position_preserve_skipped_count() {
        let mut dec = decoder();
        dec.update(false, false); // 11→00: skip
        dec.reset();
        assert_eq!(dec.skipped_count(), 1);
        dec.set_position(7);
        assert_eq!(dec.skipped_count(), 1);
    }

    #[test]
    fn skipped_count_wraps_without_panicking() {
        let mut dec = decoder();
        dec.skipped = u32::MAX;
        dec.update(false, false); // 11→00: skip
        assert_eq!(dec.skipped_count(), 0);
    }

    // --- QuadratureInput (hal adapter) ---

    #[cfg(feature = "hal")]
    mod hal_tests {
        use super::super::{EncoderDirection, QuadratureInput};
        use crate::mock::MockInputPin;

        fn enc() -> QuadratureInput<MockInputPin, MockInputPin> {
            QuadratureInput::new(
                MockInputPin::new(true),
                MockInputPin::new(true),
                true,
                true,
                4,
            )
        }

        fn drive_cw(
            enc: &mut QuadratureInput<MockInputPin, MockInputPin>,
        ) -> Option<EncoderDirection> {
            // CW: 11 → 01 → 00 → 10 → 11
            enc.pin_a_mut().set_low(); // A=0, B=1 → 01
            enc.update().unwrap();
            enc.pin_b_mut().set_low(); // A=0, B=0 → 00
            enc.update().unwrap();
            enc.pin_a_mut().set_high(); // A=1, B=0 → 10
            enc.update().unwrap();
            enc.pin_b_mut().set_high(); // A=1, B=1 → 11
            enc.update().unwrap()
        }

        fn drive_ccw(
            enc: &mut QuadratureInput<MockInputPin, MockInputPin>,
        ) -> Option<EncoderDirection> {
            // CCW: 11 → 10 → 00 → 01 → 11
            enc.pin_b_mut().set_low(); // A=1, B=0 → 10
            enc.update().unwrap();
            enc.pin_a_mut().set_low(); // A=0, B=0 → 00
            enc.update().unwrap();
            enc.pin_b_mut().set_high(); // A=0, B=1 → 01
            enc.update().unwrap();
            enc.pin_a_mut().set_high(); // A=1, B=1 → 11
            enc.update().unwrap()
        }

        #[test]
        fn hal_cw_one_detent() {
            let mut enc = enc();
            assert_eq!(drive_cw(&mut enc), Some(EncoderDirection::Clockwise));
            assert_eq!(enc.position(), 1);
        }

        #[test]
        fn hal_ccw_one_detent() {
            let mut enc = enc();
            assert_eq!(
                drive_ccw(&mut enc),
                Some(EncoderDirection::CounterClockwise)
            );
            assert_eq!(enc.position(), -1);
        }

        #[test]
        fn hal_cw_then_ccw_zero() {
            let mut enc = enc();
            drive_cw(&mut enc);
            drive_ccw(&mut enc);
            assert_eq!(enc.position(), 0);
        }

        #[test]
        fn hal_set_position() {
            let mut enc = enc();
            drive_cw(&mut enc);
            enc.set_position(10);
            assert_eq!(enc.position(), 10);
        }

        /// Drives the first three CW transitions: 11 → 01 → 00 → 10.
        fn drive_cw_partial(enc: &mut QuadratureInput<MockInputPin, MockInputPin>) {
            enc.pin_a_mut().set_low();
            enc.update().unwrap();
            enc.pin_b_mut().set_low();
            enc.update().unwrap();
            enc.pin_a_mut().set_high();
            enc.update().unwrap();
        }

        /// Drives the first three CCW transitions: 11 → 10 → 00 → 01.
        fn drive_ccw_partial(enc: &mut QuadratureInput<MockInputPin, MockInputPin>) {
            enc.pin_b_mut().set_low();
            enc.update().unwrap();
            enc.pin_a_mut().set_low();
            enc.update().unwrap();
            enc.pin_b_mut().set_high();
            enc.update().unwrap();
        }

        #[test]
        fn hal_set_position_mid_detent_discards_partial_progress() {
            let mut enc = enc();
            drive_cw_partial(&mut enc);
            enc.set_position(10);
            enc.pin_b_mut().set_high(); // 10→11
            assert_eq!(enc.update().unwrap(), None);
            assert_eq!(enc.position(), 10);

            let mut enc = self::enc();
            drive_ccw_partial(&mut enc);
            enc.set_position(10);
            enc.pin_a_mut().set_high(); // 01→11
            assert_eq!(enc.update().unwrap(), None);
            assert_eq!(enc.position(), 10);
        }

        #[test]
        fn hal_reset_mid_detent_discards_partial_progress() {
            let mut enc = enc();
            drive_cw_partial(&mut enc);
            enc.reset();
            enc.pin_b_mut().set_high(); // 10→11
            assert_eq!(enc.update().unwrap(), None);
            assert_eq!(enc.position(), 0);

            let mut enc = self::enc();
            drive_ccw_partial(&mut enc);
            enc.reset();
            enc.pin_a_mut().set_high(); // 01→11
            assert_eq!(enc.update().unwrap(), None);
            assert_eq!(enc.position(), 0);
        }

        #[test]
        fn hal_position_wraps_at_both_limits() {
            let mut enc = enc();
            enc.set_position(i32::MAX);
            assert_eq!(drive_cw(&mut enc), Some(EncoderDirection::Clockwise));
            assert_eq!(enc.position(), i32::MIN);
            assert_eq!(
                drive_ccw(&mut enc),
                Some(EncoderDirection::CounterClockwise)
            );
            assert_eq!(enc.position(), i32::MAX);
        }

        #[test]
        fn hal_reset() {
            let mut enc = enc();
            drive_cw(&mut enc);
            drive_cw(&mut enc);
            enc.reset();
            assert_eq!(enc.position(), 0);
        }

        #[test]
        fn try_from_pins_seeds_initial_state_from_hardware() {
            // Both pins rest high (state 11). Seeding from them means the
            // first CW cycle starts cleanly from the resting detent.
            let mut enc =
                QuadratureInput::try_from_pins(MockInputPin::new(true), MockInputPin::new(true), 4)
                    .unwrap();
            assert_eq!(drive_cw(&mut enc), Some(EncoderDirection::Clockwise));
            assert_eq!(enc.position(), 1);
        }

        #[test]
        fn hal_skipped_count_passes_through() {
            let mut enc = enc();
            // Both pins drop at once: 11→00 is a 2-bit jump.
            enc.pin_a_mut().set_low();
            enc.pin_b_mut().set_low();
            assert_eq!(enc.update().unwrap(), None);
            assert_eq!(enc.skipped_count(), 1);
        }

        #[test]
        fn pin_shared_ref_accessors_compile() {
            let enc = enc();
            let _ = enc.pin_a();
            let _ = enc.pin_b();
        }
    }

    mod encoder_trait {
        use super::super::*;

        fn tick<E: RotaryEncoder>(e: &mut E, now: u64) -> Option<ButtonEvent> {
            e.update(now)
        }

        #[test]
        fn noop_starts_at_zero() {
            assert_eq!(NoopRotaryEncoder::new().position(), 0);
            assert_eq!(NoopRotaryEncoder::default(), NoopRotaryEncoder::new());
        }

        #[test]
        fn noop_set_position_is_observable() {
            let mut e = NoopRotaryEncoder::new();
            e.set_position(-5);
            assert_eq!(e.position(), -5);
            e.set_position(i32::MIN);
            assert_eq!(e.position(), i32::MIN);
            e.set_position(i32::MAX);
            assert_eq!(e.position(), i32::MAX);
        }

        #[test]
        fn noop_reset_zeroes_position() {
            let mut e = NoopRotaryEncoder::new();
            e.set_position(42);
            e.reset();
            assert_eq!(e.position(), 0);
        }

        #[test]
        fn noop_update_never_emits() {
            let mut e = NoopRotaryEncoder::new();
            e.set_position(3);
            for now in [0, 1, 10_000, u64::MAX] {
                assert_eq!(e.update(now), None);
                assert_eq!(e.position(), 3);
            }
        }

        #[test]
        fn noop_button_never_pressed() {
            let mut e = NoopRotaryEncoder::new();
            assert!(!e.is_button_pressed());
            let _ = e.update(5);
            e.set_position(9);
            e.reset();
            assert!(!e.is_button_pressed());
        }

        #[test]
        fn trait_is_generic_usable() {
            let mut e = NoopRotaryEncoder::new();
            assert_eq!(tick(&mut e, 0), None);
            let d: &mut dyn RotaryEncoder = &mut e;
            assert_eq!(d.position(), 0);
        }

        #[test]
        fn noop_is_copy_and_const_new() {
            const E: NoopRotaryEncoder = NoopRotaryEncoder::new();
            let a = E;
            let b = a;
            assert_eq!(a, b);
        }

        #[test]
        fn default_reset_goes_through_set_position() {
            struct Rec {
                calls: [Option<i32>; 4],
                n: usize,
            }
            impl RotaryEncoder for Rec {
                fn update(&mut self, _: u64) -> Option<ButtonEvent> {
                    None
                }
                fn position(&self) -> i32 {
                    0
                }
                fn set_position(&mut self, p: i32) {
                    self.calls[self.n] = Some(p);
                    self.n += 1;
                }
                fn is_button_pressed(&self) -> bool {
                    false
                }
            }
            let mut r = Rec {
                calls: [None; 4],
                n: 0,
            };
            r.reset();
            assert_eq!(r.calls[0], Some(0));
            assert_eq!(r.n, 1);
        }
    }
}
