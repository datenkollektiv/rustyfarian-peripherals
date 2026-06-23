//! Button-event detection — [`ButtonDecoder`] and [`ButtonEvent`].
//!
//! Turns a (possibly bouncing) momentary-button signal into a clean stream of
//! semantic events: the raw [`Press`](ButtonEvent::Press) /
//! [`Release`](ButtonEvent::Release) edges plus the higher-level
//! [`Click`](ButtonEvent::Click), [`DoubleClick`](ButtonEvent::DoubleClick),
//! and [`LongPress`](ButtonEvent::LongPress) gestures.
//!
//! # Clock
//!
//! Like the rest of `tamer`, the caller owns the clock: pass monotonic `u64`
//! tick values (milliseconds, microseconds, raw counts — your choice, kept
//! consistent between construction and updates) to every
//! [`update`](ButtonDecoder::update). All elapsed arithmetic uses
//! [`saturating_sub`](u64::saturating_sub), so a non-monotonic timestamp clamps
//! to zero rather than wrapping.
//!
//! # Debouncing
//!
//! The raw `pressed` signal is debounced by an internal
//! [`EdgeDetector`](crate::debounce::EdgeDetector): a press or release is only
//! recognised once the level has been stable for the debounce window. Gesture
//! timing (long-press, double-click) is measured from those debounced edges,
//! so the gesture layer never sees contact bounce.
//!
//! # Event sequences
//!
//! [`Press`](ButtonEvent::Press) and [`Release`](ButtonEvent::Release) are the
//! raw debounced edges and fire on *every* press and lift. The
//! [`Click`](ButtonEvent::Click), [`DoubleClick`](ButtonEvent::DoubleClick), and
//! [`LongPress`](ButtonEvent::LongPress) gestures are layered on top: a consumer
//! that only wants up/down can match `Press`/`Release` and ignore the rest.
//!
//! At most one [`ButtonEvent`] is emitted per [`update`](ButtonDecoder::update),
//! so a release that also completes a gesture emits the raw `Release` first and
//! the gesture on the *next* call. The gestures decompose into these ordered
//! sequences:
//!
//! | Gesture | Events, in order |
//! |---|---|
//! | Short tap | [`Press`](ButtonEvent::Press), [`Release`](ButtonEvent::Release), [`Click`](ButtonEvent::Click) |
//! | Double tap (both releases within the double-click window) | [`Press`](ButtonEvent::Press), [`Release`](ButtonEvent::Release), [`Click`](ButtonEvent::Click), [`Press`](ButtonEvent::Press), [`Release`](ButtonEvent::Release), [`DoubleClick`](ButtonEvent::DoubleClick) |
//! | Long press | [`Press`](ButtonEvent::Press), [`LongPress`](ButtonEvent::LongPress), [`Release`](ButtonEvent::Release) |
//!
//! [`LongPress`](ButtonEvent::LongPress) fires *while the button is still held*,
//! the moment the hold reaches the threshold, and suppresses the
//! [`Click`](ButtonEvent::Click) that the lift would otherwise queue — so a long
//! press ends with a bare [`Release`](ButtonEvent::Release), no `Click`. The raw
//! debounced level is also available any time via
//! [`is_pressed`](ButtonDecoder::is_pressed).
//!
//! # Example
//!
//! ```
//! use tamer::button::{ButtonDecoder, ButtonEvent};
//!
//! // debounce = 5, long-press = 1000, double-click = 300 (all caller ticks)
//! let mut btn = ButtonDecoder::new(false, 5, 1000, 300);
//!
//! // Press at t=0, confirmed once it has been stable for the debounce window.
//! assert_eq!(btn.update(true, 0), None);
//! assert_eq!(btn.update(true, 5), Some(ButtonEvent::Press));
//!
//! // Release fires the raw up-edge; the Click follows on the next call.
//! assert_eq!(btn.update(false, 10), None);
//! assert_eq!(btn.update(false, 15), Some(ButtonEvent::Release));
//! assert_eq!(btn.update(false, 20), Some(ButtonEvent::Click));
//! ```

use crate::debounce::{Edge, EdgeDetector};

/// A semantic button event emitted by [`ButtonDecoder::update`].
///
/// [`Press`](Self::Press) and [`Release`](Self::Release) are raw debounced edges
/// and fire on every press and lift. [`Click`](Self::Click),
/// [`DoubleClick`](Self::DoubleClick), and [`LongPress`](Self::LongPress) are
/// gestures layered on top — see the [module docs](self) for the full sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonEvent {
    /// The button was pressed (debounced press edge). Fires on every press.
    Press,
    /// The button was released (debounced release edge). Fires on every lift —
    /// for a short tap it is followed by [`Click`](Self::Click) /
    /// [`DoubleClick`](Self::DoubleClick) on the next call; for a long press it
    /// is the final, bare event.
    Release,
    /// A short tap completed: a press and release with no second tap inside the
    /// double-click window and not held long enough to be a long-press. Emitted
    /// the call *after* the [`Release`](Self::Release) that completes the tap.
    Click,
    /// A second tap whose release fell strictly within the double-click window of
    /// the previous click. Emitted the call *after* the second tap's
    /// [`Release`](Self::Release).
    DoubleClick,
    /// The button was held continuously for at least the long-press threshold.
    /// Fired once, while still held; suppresses the [`Click`](Self::Click) the
    /// lift would otherwise produce (the lift still emits [`Release`](Self::Release)).
    LongPress,
}

/// Pure button-event decoder.
///
/// Feeds a `pressed` signal plus the current time, debounces the signal, and
/// emits [`ButtonEvent`]s. It has no hardware dependency — the clock and the
/// `pressed` boolean are supplied by the caller — so it is fully host-testable.
///
/// Enable the `hal` feature for the [`ButtonInput`] adapter that reads an
/// `embedded-hal` `InputPin` directly.
///
/// # Example
///
/// ```
/// use tamer::button::{ButtonDecoder, ButtonEvent};
///
/// let mut btn = ButtonDecoder::new(false, 5, 1000, 300);
///
/// // Hold past the long-press threshold (measured from the confirmed press).
/// assert_eq!(btn.update(true, 0), None);
/// assert_eq!(btn.update(true, 5), Some(ButtonEvent::Press));
/// assert_eq!(btn.update(true, 1005), Some(ButtonEvent::LongPress));
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ButtonDecoder {
    edges: EdgeDetector,
    long_press: u64,
    double_click: u64,
    /// Timestamp of the most recent confirmed press edge.
    pressed_since: u64,
    /// Release time of the previous click, for double-click detection.
    /// `None` means "no prior click" — distinct from a click at tick `0`, which
    /// avoids misclassifying the very first tap as a double-click.
    last_click: Option<u64>,
    long_press_fired: bool,
    /// A single event queued for the next `update`. The decoder emits at most
    /// one event per call, so a release that also completes a gesture defers the
    /// gesture (or, in the coarse-poll long-press path, the `Release`) to the
    /// following call. Drained at the top of `update`.
    pending: Option<ButtonEvent>,
}

impl ButtonDecoder {
    /// Creates a new decoder.
    ///
    /// - `initial_pressed`: the logical button state at construction (`true` =
    ///   pressed). Seed it from the live debounced level to avoid an artificial
    ///   first edge. Long-press timing is well-defined only from a *confirmed*
    ///   press edge: constructing with `initial_pressed = true` treats the hold
    ///   as already consumed, so no [`LongPress`](ButtonEvent::LongPress) fires
    ///   until the next genuine press; the first lift simply emits
    ///   [`Release`](ButtonEvent::Release).
    /// - `debounce`: debounce window for the press/release signal, in caller ticks.
    ///   A `0` window means no debouncing (edges confirm on the first changed
    ///   sample). Because emitting a deferred gesture skips one debouncer tick, a
    ///   `0` window can miss a one-sample-wide press that lands exactly on that
    ///   tick; any `debounce >= 1` (which real, bouncy buttons need) reduces this
    ///   to at most a one-tick delay of the next press.
    /// - `long_press`: minimum continuous hold to emit
    ///   [`LongPress`](ButtonEvent::LongPress). A value of `0` makes every press
    ///   an immediate long-press (suppressing [`Click`](ButtonEvent::Click) and
    ///   [`DoubleClick`](ButtonEvent::DoubleClick)).
    /// - `double_click`: the second click's release must fall *strictly within*
    ///   `double_click` ticks of the first to emit
    ///   [`DoubleClick`](ButtonEvent::DoubleClick) — a gap of exactly
    ///   `double_click` ticks is a [`Click`](ButtonEvent::Click), and `0`
    ///   disables double-click detection.
    #[must_use]
    pub fn new(initial_pressed: bool, debounce: u64, long_press: u64, double_click: u64) -> Self {
        Self {
            edges: EdgeDetector::new(initial_pressed, debounce),
            long_press,
            double_click,
            pressed_since: 0,
            last_click: None,
            // A button that powers up already held is treated as
            // "long-press already consumed": the mid-hold check measures from
            // `pressed_since = 0`, so without this it would emit a phantom
            // `LongPress` (with no preceding `Press`) on the first late tick.
            // The next genuine press edge clears the flag in `on_edge`.
            long_press_fired: initial_pressed,
            pending: None,
        }
    }

    /// Feeds a raw `pressed` sample at time `now`.
    ///
    /// Returns at most one [`ButtonEvent`] per call; see the
    /// [module documentation](self) for the full event sequences.
    pub fn update(&mut self, pressed: bool, now: u64) -> Option<ButtonEvent> {
        // A release edge can complete a gesture (or, in the coarse-poll
        // long-press path, defer its own lift); deliver that queued event before
        // processing this sample. This skips one debouncer tick: with
        // `debounce >= 1` a held press is merely recognised one tick later (a
        // one-sample press would not survive the debounce window anyway), so no
        // real edge is lost. Only with `debounce == 0` can a one-sample-wide
        // press landing exactly on this tick be missed — see `ButtonDecoder::new`.
        if let Some(event) = self.pending.take() {
            return Some(event);
        }

        if let Some(edge) = self.edges.update(pressed, now) {
            return Some(self.on_edge(edge, now));
        }

        // No debounced edge this tick: emit LongPress once, when the hold
        // threshold is crossed while the button is still held.
        if self.edges.stable_state()
            && !self.long_press_fired
            && now.saturating_sub(self.pressed_since) >= self.long_press
        {
            self.long_press_fired = true;
            return Some(ButtonEvent::LongPress);
        }

        None
    }

    fn on_edge(&mut self, edge: Edge, now: u64) -> ButtonEvent {
        match edge {
            Edge::Rising => {
                self.pressed_since = now;
                self.long_press_fired = false;
                ButtonEvent::Press
            }
            Edge::Falling => {
                // Coarse polling: the hold already crossed the long-press
                // threshold but no mid-hold tick fired `LongPress`, so the press
                // and release edges land in consecutive calls. Emit `LongPress`
                // now and queue the raw `Release` for the next call.
                if !self.long_press_fired
                    && now.saturating_sub(self.pressed_since) >= self.long_press
                {
                    self.long_press_fired = true;
                    self.last_click = None;
                    self.pending = Some(ButtonEvent::Release);
                    return ButtonEvent::LongPress;
                }

                // The lift of a hold whose `LongPress` already fired mid-hold:
                // a bare `Release`, with no trailing click. Clear any stale click
                // so the next tap can't pair with one from before the long press.
                if self.long_press_fired {
                    self.last_click = None;
                    return ButtonEvent::Release;
                }

                // Short tap: the raw `Release` fires now; the gesture
                // (`Click`, or `DoubleClick` when the previous click's release
                // was strictly within the double-click window) is queued for the
                // next call.
                let is_double = self
                    .last_click
                    .is_some_and(|t0| now.saturating_sub(t0) < self.double_click);
                self.pending = Some(if is_double {
                    self.last_click = None;
                    ButtonEvent::DoubleClick
                } else {
                    self.last_click = Some(now);
                    ButtonEvent::Click
                });
                ButtonEvent::Release
            }
        }
    }

    /// Returns the current debounced (stable) pressed state.
    #[must_use]
    pub fn is_pressed(&self) -> bool {
        self.edges.stable_state()
    }
}

/// Thin `embedded-hal` adapter that drives a [`ButtonDecoder`] from a digital
/// input pin.
///
/// Reads the pin on every [`update`](ButtonInput::update), converts the level to
/// a logical `pressed` boolean according to the configured polarity, and ticks
/// the decoder.
///
/// # Example
///
/// The example uses [`MockInputPin`](crate::mock::MockInputPin) and an
/// active-low button (pressed = pin low, the common pull-up wiring).
///
/// ```
/// # #[cfg(feature = "hal")] {
/// use tamer::button::{ButtonEvent, ButtonInput};
/// use tamer::mock::MockInputPin;
///
/// // Pin starts high (released) for an active-low button.
/// let pin = MockInputPin::new(true);
/// let mut btn = ButtonInput::new(pin, true, false, 5, 1000, 300);
///
/// // Press: drive the pin low; Press confirms after the debounce window.
/// btn.pin_mut().set_low();
/// assert_eq!(btn.update(0).unwrap(), None);
/// assert_eq!(btn.update(5).unwrap(), Some(ButtonEvent::Press));
/// # }
/// ```
#[cfg(feature = "hal")]
pub struct ButtonInput<P> {
    pin: P,
    active_low: bool,
    decoder: ButtonDecoder,
}

/// Maps a raw pin level to a logical `pressed` boolean for the given polarity.
///
/// For an active-low button (`active_low == true`), a low pin reads as pressed;
/// for active-high, a high pin reads as pressed. `active_low != level` captures
/// both cases without branching.
#[cfg(feature = "hal")]
const fn level_to_pressed(active_low: bool, level: bool) -> bool {
    active_low != level
}

#[cfg(feature = "hal")]
impl<P: embedded_hal::digital::InputPin> ButtonInput<P> {
    /// Creates a new adapter.
    ///
    /// - `active_low`: `true` if the button pulls the pin low when pressed (the
    ///   common wiring with a pull-up resistor); `false` for active-high.
    /// - `initial_pressed`: logical pressed state at construction. Prefer
    ///   [`try_from_pin`](ButtonInput::try_from_pin) to seed it from the live pin.
    /// - `debounce` / `long_press` / `double_click`: passed through to
    ///   [`ButtonDecoder::new`].
    ///
    /// Note that `active_low` and `initial_pressed` are adjacent `bool`
    /// arguments and easy to transpose;
    /// [`try_from_pin`](ButtonInput::try_from_pin) avoids the second one entirely
    /// by reading the live pin.
    #[must_use]
    pub fn new(
        pin: P,
        active_low: bool,
        initial_pressed: bool,
        debounce: u64,
        long_press: u64,
        double_click: u64,
    ) -> Self {
        Self {
            pin,
            active_low,
            decoder: ButtonDecoder::new(initial_pressed, debounce, long_press, double_click),
        }
    }

    /// Creates a new adapter, seeding the initial pressed state from a live pin
    /// read. This avoids the desynchronization risk of passing an explicit
    /// `initial_pressed` to [`new`](ButtonInput::new).
    ///
    /// # Errors
    ///
    /// Returns `Err(e)` if the initial pin read fails.
    pub fn try_from_pin(
        mut pin: P,
        active_low: bool,
        debounce: u64,
        long_press: u64,
        double_click: u64,
    ) -> Result<Self, P::Error> {
        let level = pin.is_high()?;
        let pressed = level_to_pressed(active_low, level);
        Ok(Self {
            pin,
            active_low,
            decoder: ButtonDecoder::new(pressed, debounce, long_press, double_click),
        })
    }

    /// Reads the pin and ticks the internal [`ButtonDecoder`].
    ///
    /// Returns `Ok(Some(event))` on a button event, `Ok(None)` when quiet, or
    /// `Err(e)` if the pin read fails.
    pub fn update(&mut self, now: u64) -> Result<Option<ButtonEvent>, P::Error> {
        let level = self.pin.is_high()?;
        let pressed = level_to_pressed(self.active_low, level);
        Ok(self.decoder.update(pressed, now))
    }

    /// Returns the current debounced pressed state without reading the pin.
    #[must_use]
    pub fn is_pressed(&self) -> bool {
        self.decoder.is_pressed()
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

    // debounce = 5, long-press = 1000, double-click = 300 (caller ticks).
    fn btn() -> ButtonDecoder {
        ButtonDecoder::new(false, 5, 1000, 300)
    }

    #[test]
    fn press_confirms_after_debounce_window() {
        let mut b = btn();
        assert_eq!(b.update(true, 0), None); // pending, window not elapsed
        assert_eq!(b.update(true, 5), Some(ButtonEvent::Press)); // 5 - 0 >= 5
        assert!(b.is_pressed());
    }

    #[test]
    fn short_tap_emits_press_release_click() {
        let mut b = btn();
        assert_eq!(b.update(true, 0), None);
        assert_eq!(b.update(true, 5), Some(ButtonEvent::Press));
        assert_eq!(b.update(false, 10), None);
        // Raw up-edge fires, then the Click on the next call.
        assert_eq!(b.update(false, 15), Some(ButtonEvent::Release));
        assert_eq!(b.update(false, 16), Some(ButtonEvent::Click));
        assert!(!b.is_pressed());
    }

    #[test]
    fn first_tap_is_never_misclassified_as_double_click() {
        // Regression guard ported from the knob: a first release within the
        // double-click window of "tick 0" must be a Click, not a DoubleClick.
        let mut b = btn();
        b.update(true, 0);
        b.update(true, 5); // Press
        b.update(false, 10);
        // Release confirmed at t=15, well within double_click=300 of t=0.
        assert_eq!(b.update(false, 15), Some(ButtonEvent::Release));
        assert_eq!(b.update(false, 16), Some(ButtonEvent::Click));
    }

    #[test]
    fn double_click_within_window_fires_double_click() {
        let mut b = btn();
        // First tap → Release then Click at t=15 (last_click = 15).
        b.update(true, 0);
        b.update(true, 5);
        b.update(false, 10);
        assert_eq!(b.update(false, 15), Some(ButtonEvent::Release));
        assert_eq!(b.update(false, 16), Some(ButtonEvent::Click));
        // Second tap released within 300 of the first (before t=315).
        b.update(true, 20);
        assert_eq!(b.update(true, 25), Some(ButtonEvent::Press));
        b.update(false, 30);
        assert_eq!(b.update(false, 35), Some(ButtonEvent::Release));
        assert_eq!(b.update(false, 36), Some(ButtonEvent::DoubleClick)); // 35 - 15 < 300
    }

    #[test]
    fn second_tap_outside_window_fires_two_clicks() {
        let mut b = btn();
        b.update(true, 0);
        b.update(true, 5);
        b.update(false, 10);
        assert_eq!(b.update(false, 15), Some(ButtonEvent::Release));
        assert_eq!(b.update(false, 16), Some(ButtonEvent::Click)); // last_click = 15
                                                                   // Second release after the window (t=415; 415 - 15 = 400 >= 300).
        b.update(true, 400);
        assert_eq!(b.update(true, 405), Some(ButtonEvent::Press));
        b.update(false, 410);
        assert_eq!(b.update(false, 415), Some(ButtonEvent::Release));
        assert_eq!(b.update(false, 416), Some(ButtonEvent::Click));
    }

    #[test]
    fn double_click_exact_boundary_is_click() {
        // A second release at exactly `last_click + double_click` is a Click,
        // not a DoubleClick (the window comparison is strict `<`).
        let mut b = btn();
        b.update(true, 0);
        b.update(true, 5);
        b.update(false, 10);
        assert_eq!(b.update(false, 15), Some(ButtonEvent::Release));
        assert_eq!(b.update(false, 16), Some(ButtonEvent::Click)); // last_click = 15
        b.update(true, 305);
        assert_eq!(b.update(true, 310), Some(ButtonEvent::Press));
        b.update(false, 310);
        assert_eq!(b.update(false, 315), Some(ButtonEvent::Release));
        // 315 - 15 == 300 == double_click, NOT < 300 → Click.
        assert_eq!(b.update(false, 316), Some(ButtonEvent::Click));
    }

    #[test]
    fn long_press_fires_mid_hold_then_release_on_lift() {
        let mut b = btn();
        assert_eq!(b.update(true, 0), None);
        assert_eq!(b.update(true, 5), Some(ButtonEvent::Press)); // pressed_since = 5
        assert_eq!(b.update(true, 500), None); // 500 - 5 < 1000
        assert_eq!(b.update(true, 1005), Some(ButtonEvent::LongPress)); // 1005 - 5 >= 1000
        assert_eq!(b.update(true, 1100), None); // fires only once
                                                // Lift after the long press → Release (no Click).
        assert_eq!(b.update(false, 1200), None);
        assert_eq!(b.update(false, 1205), Some(ButtonEvent::Release));
    }

    #[test]
    fn long_press_suppresses_click() {
        // After a LongPress, the release must not be classified as a Click,
        // and must not seed a double-click.
        let mut b = btn();
        b.update(true, 0);
        b.update(true, 5);
        b.update(true, 1005); // LongPress
        b.update(false, 1200);
        assert_eq!(b.update(false, 1205), Some(ButtonEvent::Release));
        // A following genuine short tap is a Click, not a DoubleClick.
        b.update(true, 1300);
        b.update(true, 1305); // Press
        b.update(false, 1310);
        assert_eq!(b.update(false, 1315), Some(ButtonEvent::Release));
        assert_eq!(b.update(false, 1316), Some(ButtonEvent::Click));
    }

    #[test]
    fn single_edge_long_hold_is_long_press_not_click() {
        // With a zero debounce window each edge confirms on its first sample, so
        // the press and release are observed in consecutive calls with no
        // intervening mid-hold tick. The release edge itself must be recognised
        // as a long-press (not a click) because the hold already crossed the
        // threshold — this exercises the `on_edge` Falling-edge guard.
        let mut b = ButtonDecoder::new(false, 0, 1000, 300);
        assert_eq!(b.update(true, 5), Some(ButtonEvent::Press)); // pressed_since = 5
        assert_eq!(b.update(false, 2005), Some(ButtonEvent::LongPress)); // 2005 - 5 >= 1000
                                                                         // The lift collapsed into the release edge is queued and delivered on
                                                                         // the next call, completing Press → LongPress → Release.
        assert_eq!(b.update(false, 2010), Some(ButtonEvent::Release));
        assert_eq!(b.update(false, 2015), None);
    }

    #[test]
    fn constructed_pressed_does_not_emit_phantom_long_press() {
        // Regression (B1): a decoder constructed as already-pressed must not
        // retroactively emit a LongPress measured from tick 0 when the caller's
        // clock starts far from zero.
        let mut b = ButtonDecoder::new(true, 5, 1000, 300);
        assert!(b.is_pressed());
        assert_eq!(b.update(true, 5000), None); // no phantom LongPress
                                                // Releasing the construction-time hold reads as a bare Release (the hold
                                                // is treated as already consumed), not a Click.
        assert_eq!(b.update(false, 5005), None);
        assert_eq!(b.update(false, 5010), Some(ButtonEvent::Release));
        // A subsequent genuine short tap then behaves normally.
        b.update(true, 6000);
        assert_eq!(b.update(true, 6005), Some(ButtonEvent::Press));
        b.update(false, 6010);
        assert_eq!(b.update(false, 6015), Some(ButtonEvent::Release));
        assert_eq!(b.update(false, 6016), Some(ButtonEvent::Click));
    }

    #[test]
    fn tap_after_long_press_is_click_not_double_click() {
        // Regression (B2): a long press must clear the prior click, so a short
        // tap after it cannot pair into a DoubleClick even with a wide window.
        let mut b = ButtonDecoder::new(false, 5, 1000, 10_000);
        // First short tap → Release then Click (last_click recorded).
        b.update(true, 0);
        b.update(true, 5);
        b.update(false, 10);
        assert_eq!(b.update(false, 15), Some(ButtonEvent::Release));
        assert_eq!(b.update(false, 16), Some(ButtonEvent::Click));
        // Long press intervenes.
        b.update(true, 100);
        b.update(true, 105); // Press
        assert_eq!(b.update(true, 1105), Some(ButtonEvent::LongPress));
        b.update(false, 1200);
        assert_eq!(b.update(false, 1205), Some(ButtonEvent::Release));
        // Following short tap: released at 1310, only ~1295 ticks after the
        // first click at 15 — inside the 10_000 window, yet must NOT double.
        b.update(true, 1300);
        b.update(true, 1305); // Press
        b.update(false, 1310);
        assert_eq!(b.update(false, 1315), Some(ButtonEvent::Release));
        assert_eq!(b.update(false, 1316), Some(ButtonEvent::Click));
    }

    #[test]
    fn debounced_long_hold_fires_long_press_during_release_window() {
        // With a non-zero debounce window, the release spends its debounce
        // window pending while the button still reads as stably pressed, so the
        // mid-hold check fires LongPress before the release is confirmed; the
        // confirmed release then reports Release.
        let mut b = btn();
        b.update(true, 0);
        assert_eq!(b.update(true, 5), Some(ButtonEvent::Press)); // pressed_since = 5
        assert_eq!(b.update(false, 2000), Some(ButtonEvent::LongPress)); // mid-hold detection
        assert_eq!(b.update(false, 2005), Some(ButtonEvent::Release)); // confirmed release
    }

    #[test]
    fn stable_held_produces_no_event_before_threshold() {
        let mut b = btn();
        b.update(true, 0);
        b.update(true, 5); // Press; threshold at t=1005
        for t in (10u64..1000).step_by(37) {
            assert_eq!(b.update(true, t), None);
        }
    }

    #[test]
    fn stable_released_produces_no_event() {
        let mut b = btn();
        for t in (0u64..500).step_by(10) {
            assert_eq!(b.update(false, t), None);
        }
        assert!(!b.is_pressed());
    }

    #[test]
    fn is_pressed_tracks_debounced_state() {
        let mut b = btn();
        assert!(!b.is_pressed());
        b.update(true, 0);
        assert!(!b.is_pressed()); // not yet confirmed
        b.update(true, 5);
        assert!(b.is_pressed()); // confirmed
    }

    #[test]
    fn every_press_is_bracketed_by_release() {
        // Raw edges are symmetric: across a short tap and a long press, each
        // Press is matched by exactly one Release.
        let mut b = btn();
        let samples: &[(bool, u64)] = &[
            (true, 0),
            (true, 5), // press
            (false, 10),
            (false, 15),
            (false, 16), // release + click
            (true, 100),
            (true, 105),  // press
            (true, 1105), // long press
            (false, 1200),
            (false, 1205),
            (false, 1206), // release
        ];
        let mut events = Vec::new();
        for &(pressed, now) in samples {
            if let Some(event) = b.update(pressed, now) {
                events.push(event);
            }
        }
        let presses = events.iter().filter(|&&e| e == ButtonEvent::Press).count();
        let releases = events
            .iter()
            .filter(|&&e| e == ButtonEvent::Release)
            .count();
        assert_eq!(presses, 2);
        assert_eq!(
            presses, releases,
            "every Press must have a matching Release"
        );
    }

    #[test]
    fn press_during_gesture_drain_is_delayed_not_lost() {
        // With debounce >= 1, a press arriving on the tick the queued Click is
        // drained is not lost: the dropped sample is re-read next call and the
        // held press is recognised one debouncer tick later.
        let mut b = btn(); // debounce = 5
        b.update(true, 0);
        assert_eq!(b.update(true, 5), Some(ButtonEvent::Press));
        b.update(false, 10);
        assert_eq!(b.update(false, 15), Some(ButtonEvent::Release)); // pending = Click
                                                                     // Re-press exactly on the Click-drain tick: Click drains, sample skipped.
        assert_eq!(b.update(true, 16), Some(ButtonEvent::Click));
        // The still-held press confirms a debounce window after it is re-read.
        assert_eq!(b.update(true, 17), None);
        assert_eq!(b.update(true, 22), Some(ButtonEvent::Press));
    }

    #[test]
    fn zero_debounce_one_sample_press_on_drain_tick_is_missed() {
        // Documents a known limitation (not a regression): with debounce == 0, a
        // one-sample-wide press that lands exactly on a gesture-drain tick is
        // skipped, because the drain returns the queued event without feeding the
        // sample. Real buttons use debounce >= 1, where a press is at most delayed
        // by a tick (see `press_during_gesture_drain_is_delayed_not_lost`).
        let mut b = ButtonDecoder::new(false, 0, 1000, 300);
        assert_eq!(b.update(true, 0), Some(ButtonEvent::Press));
        assert_eq!(b.update(false, 1), Some(ButtonEvent::Release)); // pending = Click
                                                                    // One-sample press at t=2 (the Click-drain tick), released again at t=3.
        assert_eq!(b.update(true, 2), Some(ButtonEvent::Click)); // drained; press skipped
        assert_eq!(b.update(false, 3), None); // the press was never observed
        assert!(!b.is_pressed());
    }

    // --- ButtonInput (hal adapter) ---

    #[cfg(feature = "hal")]
    mod hal_tests {
        use super::super::{ButtonEvent, ButtonInput};
        use crate::mock::MockInputPin;

        // Active-low button: pin high = released, pin low = pressed.
        fn active_low_btn() -> ButtonInput<MockInputPin> {
            ButtonInput::new(MockInputPin::new(true), true, false, 5, 1000, 300)
        }

        #[test]
        fn active_low_press_release_click() {
            let mut b = active_low_btn();
            b.pin_mut().set_low(); // press
            assert_eq!(b.update(0).unwrap(), None);
            assert_eq!(b.update(5).unwrap(), Some(ButtonEvent::Press));
            assert!(b.is_pressed());

            b.pin_mut().set_high(); // release
            assert_eq!(b.update(10).unwrap(), None);
            assert_eq!(b.update(15).unwrap(), Some(ButtonEvent::Release));
            assert_eq!(b.update(20).unwrap(), Some(ButtonEvent::Click));
            assert!(!b.is_pressed());
        }

        #[test]
        fn active_high_press_release_click() {
            // Active-high: pin high = pressed.
            let mut b = ButtonInput::new(MockInputPin::new(false), false, false, 5, 1000, 300);
            b.pin_mut().set_high(); // press
            assert_eq!(b.update(0).unwrap(), None);
            assert_eq!(b.update(5).unwrap(), Some(ButtonEvent::Press));
            b.pin_mut().set_low(); // release
            assert_eq!(b.update(10).unwrap(), None);
            assert_eq!(b.update(15).unwrap(), Some(ButtonEvent::Release));
            assert_eq!(b.update(20).unwrap(), Some(ButtonEvent::Click));
        }

        #[test]
        fn try_from_pin_seeds_released_state_for_active_low() {
            // Pin high at construction = released for an active-low button.
            let b = ButtonInput::try_from_pin(MockInputPin::new(true), true, 5, 1000, 300).unwrap();
            assert!(!b.is_pressed());
        }

        #[test]
        fn pin_accessor_compiles() {
            let b = active_low_btn();
            let _ = b.pin();
        }
    }
}
