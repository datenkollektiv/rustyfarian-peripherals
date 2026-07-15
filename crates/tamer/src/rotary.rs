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

/// The direction of a confirmed encoder detent step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncoderDirection {
    /// The encoder moved clockwise (position increased by 1).
    Clockwise,
    /// The encoder moved counter-clockwise (position decreased by 1).
    CounterClockwise,
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
        }
    }

    /// Feeds a new A/B sample.
    ///
    /// Returns `Some(EncoderDirection::Clockwise)` or
    /// `Some(EncoderDirection::CounterClockwise)` when the accumulator
    /// reaches a full detent.
    /// Returns `None` for partial steps, bounce, or invalid transitions.
    pub fn update(&mut self, a: bool, b: bool) -> Option<EncoderDirection> {
        let state = (a as u8) << 1 | b as u8;
        let step = QUAD_TABLE[(self.last_state << 2 | state) as usize];
        self.last_state = state;
        self.accumulator += i32::from(step);

        if self.accumulator >= self.steps_per_detent {
            self.position += 1;
            self.accumulator = 0;
            Some(EncoderDirection::Clockwise)
        } else if self.accumulator <= -self.steps_per_detent {
            self.position -= 1;
            self.accumulator = 0;
            Some(EncoderDirection::CounterClockwise)
        } else {
            None
        }
    }

    /// Returns the absolute position (increases CW, decreases CCW).
    #[must_use]
    pub fn position(&self) -> i32 {
        self.position
    }

    /// Overwrites the absolute position and clears the accumulator.
    pub fn set_position(&mut self, position: i32) {
        self.position = position;
        self.accumulator = 0;
    }

    /// Resets the position to zero and clears the accumulator.
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
    fn set_position_updates_and_clears_accumulator() {
        let mut dec = decoder();
        one_cw(&mut dec);
        dec.set_position(42);
        assert_eq!(dec.position(), 42);
        // A partial step should not add a stale accumulator offset.
        dec.update(false, true); // partial step — acc = 1, not a full detent
        assert_eq!(dec.position(), 42);
    }

    #[test]
    fn reset_zeroes_position_and_accumulator() {
        let mut dec = decoder();
        one_cw(&mut dec);
        one_cw(&mut dec);
        dec.reset();
        assert_eq!(dec.position(), 0);
        // After reset a partial step should not register.
        dec.update(false, true); // acc = 1 (not a full detent)
        assert_eq!(dec.position(), 0);
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
        fn pin_shared_ref_accessors_compile() {
            let enc = enc();
            let _ = enc.pin_a();
            let _ = enc.pin_b();
        }
    }
}
