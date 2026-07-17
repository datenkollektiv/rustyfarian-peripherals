//! Touch-event detection — [`TouchTracker`], [`TouchEvent`], [`TouchPoint`],
//! and [`SwipeDirection`].
//!
//! Turns a per-frame touch sample — `Some(point)` while touched, `None` when
//! not — into a clean stream of semantic events: the raw
//! [`Down`](TouchEvent::Down) / [`Move`](TouchEvent::Move) /
//! [`Up`](TouchEvent::Up) contact edges plus the higher-level
//! [`Tap`](TouchEvent::Tap), [`LongPress`](TouchEvent::LongPress), and
//! [`Swipe`](TouchEvent::Swipe) gestures. Gestures are derived purely from
//! motion and timing, so they work on controllers with no hardware gesture
//! engine at all (the XPT2046 being the concrete case).
//!
//! # Clock
//!
//! Like the rest of `tamer`, the caller owns the clock: pass monotonic `u64`
//! tick values (milliseconds, microseconds, raw counts — your choice, kept
//! consistent between construction and updates) to every
//! [`update`](TouchTracker::update). All elapsed arithmetic uses
//! [`saturating_sub`](u64::saturating_sub), so a non-monotonic timestamp
//! clamps to zero rather than wrapping.
//!
//! # Coordinates and composition
//!
//! There is no `hal` adapter and no touch trait: a touch panel is a bus
//! device with no `embedded-hal` trait, so the `(touch, now)` call *is* the
//! seam — the chip driver decodes its own packet and feeds points in, the
//! same HAL-agnostic precedent as [`hall`](crate::hall). The tracker consumes
//! already-calibrated, already-rotated display coordinates; calibration and
//! rotation are per-board chip-tier concerns, and swipe directions are
//! defined in the fed coordinate space:
//!
//! ```text
//! bus read (I²C/SPI) → chip decode → median filter (resistive) → calibrate + rotate
//!   → touched → Debouncer (resistive only) → TouchTracker::update → Option<TouchEvent>
//! ```
//!
//! On untouched frames pass `None` — controllers like the XPT2046 report
//! garbage coordinates at pen-up, and `Option` makes them unrepresentable.
//! The tracker carries the last *reported* point (the Down origin,
//! re-anchored by each emitted [`Move`](TouchEvent::Move)) for lift
//! classification — sub-epsilon jitter after the final `Move` is discarded,
//! so [`Up`](TouchEvent::Up) can lag the true finger position by at most
//! `move_epsilon`.
//!
//! # Event sequences
//!
//! At most one [`TouchEvent`] is emitted per [`update`](TouchTracker::update):
//! a lift emits the raw [`Up`](TouchEvent::Up) first and queues the terminal
//! gesture for the *next* call — so keep polling after `Up`, or the gesture
//! is never observed. The gestures decompose into these ordered sequences:
//!
//! | Gesture | Events, in order |
//! |---|---|
//! | Tap | [`Down`](TouchEvent::Down), [`Up`](TouchEvent::Up), [`Tap`](TouchEvent::Tap) |
//! | Swipe | [`Down`](TouchEvent::Down), [`Move`](TouchEvent::Move)…, [`Up`](TouchEvent::Up), [`Swipe`](TouchEvent::Swipe) |
//! | Long press | [`Down`](TouchEvent::Down), [`LongPress`](TouchEvent::LongPress) (mid-hold), [`Move`](TouchEvent::Move)…, [`Up`](TouchEvent::Up) |
//!
//! [`LongPress`](TouchEvent::LongPress) fires *while the finger is still
//! down*, the moment the unmoved hold reaches the threshold, and suppresses
//! the [`Tap`](TouchEvent::Tap) or [`Swipe`](TouchEvent::Swipe) the lift
//! would otherwise queue — the touch ends with a bare [`Up`](TouchEvent::Up).
//! [`Move`](TouchEvent::Move)s remain legal between `LongPress` and `Up`
//! (long-press-then-drag UIs consume them). A drag past `move_epsilon` but
//! short of `swipe_min_distance` also ends with a bare `Up`.
//!
//! # Example
//!
//! A quick tap:
//!
//! ```
//! use tamer::touch::{TouchEvent, TouchPoint, TouchTracker};
//!
//! // long_press = 600, swipe_min_distance = 50, move_epsilon = 10
//! // (caller ticks for time; display pixels for distances)
//! let mut tracker = TouchTracker::new(600, 50, 10);
//!
//! let p = TouchPoint { x: 120, y: 80 };
//! assert_eq!(tracker.update(Some(p), 0), Some(TouchEvent::Down(p)));
//! assert_eq!(tracker.update(Some(p), 20), None);
//!
//! // The lift fires the raw up-edge; the Tap follows on the next call.
//! assert_eq!(tracker.update(None, 40), Some(TouchEvent::Up(p)));
//! assert_eq!(tracker.update(None, 60), Some(TouchEvent::Tap(p)));
//! ```
//!
//! A horizontal swipe — `Move`s re-anchor along the way, and the net
//! origin-to-lift delta selects the direction:
//!
//! ```
//! use tamer::touch::{SwipeDirection, TouchEvent, TouchPoint, TouchTracker};
//!
//! let mut tracker = TouchTracker::new(600, 50, 10);
//!
//! let start = TouchPoint { x: 10, y: 100 };
//! let mid = TouchPoint { x: 45, y: 100 };
//! let end = TouchPoint { x: 80, y: 100 };
//!
//! assert_eq!(tracker.update(Some(start), 0), Some(TouchEvent::Down(start)));
//! assert_eq!(tracker.update(Some(mid), 20), Some(TouchEvent::Move(mid)));
//! assert_eq!(tracker.update(Some(end), 40), Some(TouchEvent::Move(end)));
//! assert_eq!(tracker.update(None, 60), Some(TouchEvent::Up(end)));
//! assert_eq!(tracker.update(None, 80), Some(TouchEvent::Swipe(SwipeDirection::Right)));
//! ```
//!
//! # Debouncing a resistive `touched` flag
//!
//! Resistive controllers (e.g. the CYD's XPT2046) flicker their touched
//! state; compose a [`Debouncer`](crate::debounce::Debouncer) upstream rather
//! than looking for debounce configuration here. Note that
//! [`Debouncer::update`](crate::debounce::Debouncer::update) returns
//! *transitions*, not the level — feed it, then read the debounced level from
//! [`stable_state`](crate::debounce::Debouncer::stable_state):
//!
//! ```
//! use tamer::debounce::Debouncer;
//! use tamer::touch::{TouchEvent, TouchPoint, TouchTracker};
//!
//! let mut deb = Debouncer::new(false, 30); // 30-tick touched debounce
//! let mut tracker = TouchTracker::new(600, 50, 10);
//!
//! // Each frame: raw touched flag + decoded point from the chip driver.
//! let point = TouchPoint { x: 160, y: 120 };
//!
//! // First touched frame: not yet stable — the tracker sees no touch.
//! deb.update(true, 0);
//! assert_eq!(tracker.update(deb.stable_state().then_some(point), 0), None);
//!
//! // Still touched past the window: the debounced level flips on.
//! deb.update(true, 35);
//! assert_eq!(
//!     tracker.update(deb.stable_state().then_some(point), 35),
//!     Some(TouchEvent::Down(point))
//! );
//! ```

/// A touch position in already-calibrated, already-rotated display
/// coordinates.
///
/// The tracker never sees raw ADC values or panel rotation — the chip/board
/// tier maps its readings into display units before feeding the tracker, and
/// swipe directions are defined in this fed coordinate space (`y` grows
/// downward, as usual for screens).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TouchPoint {
    /// Horizontal coordinate, growing to the right.
    pub x: i16,
    /// Vertical coordinate, growing downward.
    pub y: i16,
}

/// The direction of a completed [`Swipe`](TouchEvent::Swipe), in the fed
/// (display) coordinate space.
///
/// `y` grows downward (screen coordinates), so a drag toward larger `y` is a
/// [`Down`](Self::Down) swipe. The dominant axis selects the direction; when
/// the net deltas tie (`|dx| == |dy|`), the horizontal direction wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwipeDirection {
    /// Toward smaller `y` (the top of the screen).
    Up,
    /// Toward larger `y` (the bottom of the screen).
    Down,
    /// Toward smaller `x` (the left edge).
    Left,
    /// Toward larger `x` (the right edge).
    Right,
}

/// A semantic touch event emitted by [`TouchTracker::update`].
///
/// [`Down`](Self::Down), [`Move`](Self::Move), and [`Up`](Self::Up) are the
/// raw contact edges and fire on every touch. [`Tap`](Self::Tap),
/// [`LongPress`](Self::LongPress), and [`Swipe`](Self::Swipe) are gestures
/// layered on top — see the [module docs](self) for the full sequences.
///
/// Events carry their location because the one-deep pending queue delivers a
/// terminal gesture one call *after* the [`Up`](Self::Up), when the tracker
/// is already idle and [`position`](TouchTracker::position) is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TouchEvent {
    /// A finger made contact. Fires on every touch; never accompanied by a
    /// [`Move`](Self::Move) on the same call.
    Down(TouchPoint),
    /// The touch moved more than `move_epsilon` (Chebyshev) away from the
    /// last reported point and re-anchored there — a natural rate throttle:
    /// jitter within the epsilon emits nothing.
    Move(TouchPoint),
    /// The finger lifted. Carries the last *reported* point (the Down
    /// origin, re-anchored by each emitted [`Move`](Self::Move)); jitter
    /// within `move_epsilon` after the final `Move` is discarded. For a tap
    /// or swipe it is followed by the gesture on the next call; after a
    /// [`LongPress`](Self::LongPress) it is the final, bare event.
    Up(TouchPoint),
    /// A touch and release that never strayed beyond `move_epsilon` from the
    /// origin, with no [`LongPress`](Self::LongPress) fired. Carries the Down
    /// origin; emitted the call *after* the [`Up`](Self::Up).
    Tap(TouchPoint),
    /// The touch was held for at least the long-press threshold without
    /// straying beyond `move_epsilon`. Fired once, normally mid-hold (under
    /// coarse polling, at the lift, with the [`Up`](Self::Up) queued);
    /// carries the Down origin. Suppresses the release gesture (no
    /// [`Tap`](Self::Tap) or [`Swipe`](Self::Swipe)) but not intervening
    /// [`Move`](Self::Move)s.
    LongPress(TouchPoint),
    /// The net delta from the Down origin to the last reported point reached
    /// `swipe_min_distance` on the dominant axis at lift. Emitted the call
    /// *after* the [`Up`](Self::Up).
    Swipe(SwipeDirection),
}

/// Internal tracker state. A two-variant enum keeps invalid combinations
/// unrepresentable: the per-touch bookkeeping exists only while a touch is in
/// progress.
#[derive(Debug, Clone, Copy)]
enum State {
    /// No finger on the panel.
    Idle,
    /// A touch in progress.
    Touched {
        /// Timestamp of the [`Down`](TouchEvent::Down) edge.
        down_at: u64,
        /// The Down point; gestures classify against it.
        origin: TouchPoint,
        /// The last *reported* point: the origin, re-anchored by each emitted
        /// [`Move`](TouchEvent::Move). Carried by [`Up`](TouchEvent::Up) and
        /// used for swipe classification.
        last: TouchPoint,
        /// Latched once the touch strays beyond `move_epsilon` (Chebyshev)
        /// from `origin`; never resets during the touch — permanently cancels
        /// [`Tap`](TouchEvent::Tap) and [`LongPress`](TouchEvent::LongPress).
        moved: bool,
        /// A [`LongPress`](TouchEvent::LongPress) was already emitted for
        /// this touch: it fires at most once and suppresses the release
        /// gesture.
        long_press_fired: bool,
    },
}

/// Pure touch-event tracker.
///
/// Feed one sample per poll frame — `Some(point)` while touched, `None` when
/// not — plus the current time, and receive [`TouchEvent`]s. It has no
/// hardware dependency — the clock and the touch sample are supplied by the
/// caller — so it is fully host-testable. There is no `hal` adapter: a touch
/// panel is a bus device with no `embedded-hal` trait, so the chip driver
/// feeds decoded, calibrated points directly into
/// [`update`](TouchTracker::update).
///
/// # Polling contract
///
/// [`update`](TouchTracker::update) returns **at most one event per call**, and
/// terminal gestures arrive one call *late*: a lift emits
/// [`Up`](TouchEvent::Up) on the frame it happens and *queues* the
/// [`Tap`](TouchEvent::Tap) / [`Swipe`](TouchEvent::Swipe) for the next call —
/// a loop that stops polling the instant it sees `Up` never observes the
/// gesture. For the same reason a fresh contact landing on the exact frame a
/// queued gesture drains is deferred by one call (its origin then anchors at
/// the following frame's point). Keep polling at a steady rate; both known
/// consumers poll every 10–20 ms.
///
/// # Example
///
/// ```
/// use tamer::touch::{TouchEvent, TouchPoint, TouchTracker};
///
/// let mut tracker = TouchTracker::new(600, 50, 10);
///
/// let p = TouchPoint { x: 100, y: 100 };
/// assert_eq!(tracker.update(Some(p), 0), Some(TouchEvent::Down(p)));
///
/// // Held without movement past the threshold: LongPress fires mid-hold.
/// assert_eq!(tracker.update(Some(p), 600), Some(TouchEvent::LongPress(p)));
///
/// // The lift is a bare Up — a fired LongPress suppresses Tap and Swipe.
/// assert_eq!(tracker.update(None, 700), Some(TouchEvent::Up(p)));
/// assert_eq!(tracker.update(None, 720), None);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct TouchTracker {
    long_press: u64,
    swipe_min_distance: u16,
    move_epsilon: u16,
    state: State,
    /// A single event queued for the next `update`. The tracker emits at most
    /// one event per call, so a lift that also completes a gesture defers the
    /// gesture (or, in the coarse-poll long-press path, the `Up`) to the
    /// following call. Drained at the top of `update`.
    pending: Option<TouchEvent>,
}

/// Chebyshev (chessboard) distance between two points: the larger of the
/// absolute per-axis deltas. Computed in `i32`, where the full `i16` span
/// always fits, so no coordinate pair can overflow.
fn chebyshev(a: TouchPoint, b: TouchPoint) -> u32 {
    let dx = (i32::from(a.x) - i32::from(b.x)).unsigned_abs();
    let dy = (i32::from(a.y) - i32::from(b.y)).unsigned_abs();
    dx.max(dy)
}

/// Classifies the net `origin → end` delta into a swipe direction: the
/// dominant axis wins and a tie goes horizontal. `y` grows downward (screen
/// coordinates), so a positive `dy` is a downward swipe.
fn swipe_direction(origin: TouchPoint, end: TouchPoint) -> SwipeDirection {
    let dx = i32::from(end.x) - i32::from(origin.x);
    let dy = i32::from(end.y) - i32::from(origin.y);
    if dy.unsigned_abs() > dx.unsigned_abs() {
        if dy > 0 {
            SwipeDirection::Down
        } else {
            SwipeDirection::Up
        }
    } else if dx > 0 {
        SwipeDirection::Right
    } else {
        SwipeDirection::Left
    }
}

impl TouchTracker {
    /// Creates a new tracker, starting idle (no touch in progress).
    ///
    /// - `long_press`: minimum continuous hold, in caller ticks, to emit
    ///   [`LongPress`](TouchEvent::LongPress) (the boundary is inclusive).
    ///   Movement beyond `move_epsilon` before the threshold permanently
    ///   cancels it. A value of `0` makes every touch an immediate
    ///   long-press (suppressing [`Tap`](TouchEvent::Tap) and
    ///   [`Swipe`](TouchEvent::Swipe)).
    /// - `swipe_min_distance`: minimum net delta, in display units, from the
    ///   Down origin to the last reported point at lift (dominant axis,
    ///   inclusive) to classify the release as a
    ///   [`Swipe`](TouchEvent::Swipe). Assumed `>= move_epsilon`; this is
    ///   documented, not asserted — an inverted configuration is odd but
    ///   deterministic (Swipe wins at release). A value of `0` classifies
    ///   every release as a swipe, including a zero-delta one (tie →
    ///   horizontal, non-positive `dx` → [`SwipeDirection::Left`]).
    /// - `move_epsilon`: jitter radius in display units (Chebyshev: the
    ///   larger absolute per-axis delta). Straying *strictly* beyond it from
    ///   the Down origin cancels [`Tap`](TouchEvent::Tap) and
    ///   [`LongPress`](TouchEvent::LongPress) — it is effectively the tap
    ///   tolerance — and each point strictly beyond it from the last
    ///   reported point emits a re-anchoring [`Move`](TouchEvent::Move).
    #[must_use]
    pub fn new(long_press: u64, swipe_min_distance: u16, move_epsilon: u16) -> Self {
        Self {
            long_press,
            swipe_min_distance,
            move_epsilon,
            state: State::Idle,
            pending: None,
        }
    }

    /// Feeds one touch frame at time `now`: `Some(point)` while touched (in
    /// calibrated display coordinates), `None` when not — untouched frames
    /// carry no point at all, so garbage pen-up coordinates are
    /// unrepresentable.
    ///
    /// Returns at most one [`TouchEvent`] per call; a lift emits
    /// [`Up`](TouchEvent::Up) immediately and queues the terminal gesture
    /// for the next call, so keep polling to drain it. See the
    /// [module documentation](self) for the full event sequences.
    pub fn update(&mut self, touch: Option<TouchPoint>, now: u64) -> Option<TouchEvent> {
        // A lift queues its terminal gesture (or, in the coarse-poll
        // long-press path, the `Up` itself); deliver that queued event before
        // processing this frame. A touch landing exactly on a drain tick is
        // recognised one call later — and unlike `ButtonDecoder`, which
        // re-reads a held level, the skipped frame's coordinates are gone:
        // the origin anchors at the *next* frame's point.
        if let Some(event) = self.pending.take() {
            return Some(event);
        }

        match (self.state, touch) {
            (State::Idle, None) => None,
            (State::Idle, Some(point)) => {
                self.state = State::Touched {
                    down_at: now,
                    origin: point,
                    last: point,
                    moved: false,
                    long_press_fired: false,
                };
                Some(TouchEvent::Down(point))
            }
            (
                State::Touched {
                    down_at,
                    origin,
                    mut last,
                    mut moved,
                    mut long_press_fired,
                },
                Some(point),
            ) => {
                let epsilon = u32::from(self.move_epsilon);

                // Excursion beyond epsilon from the origin latches `moved`
                // (permanently cancelling Tap and LongPress), even when the
                // Move throttle below stays quiet this call.
                if !moved && chebyshev(point, origin) > epsilon {
                    moved = true;
                }

                let long_press_due = now.saturating_sub(down_at) >= self.long_press;
                let event = if !long_press_fired && !moved && long_press_due {
                    long_press_fired = true;
                    Some(TouchEvent::LongPress(origin))
                } else if chebyshev(point, last) > epsilon {
                    // Move re-anchors to the reported point — a natural rate
                    // throttle: jitter within epsilon emits nothing.
                    last = point;
                    Some(TouchEvent::Move(point))
                } else {
                    None
                };

                self.state = State::Touched {
                    down_at,
                    origin,
                    last,
                    moved,
                    long_press_fired,
                };
                event
            }
            (
                State::Touched {
                    down_at,
                    origin,
                    last,
                    moved,
                    long_press_fired,
                },
                None,
            ) => {
                self.state = State::Idle;

                // Coarse polling: the unmoved hold crossed the long-press
                // threshold but no mid-hold tick fired `LongPress`, so the
                // hold and the lift land in consecutive calls. Emit
                // `LongPress` now and queue the raw `Up`; the release gesture
                // is then suppressed as usual.
                if !long_press_fired && !moved && now.saturating_sub(down_at) >= self.long_press {
                    self.pending = Some(TouchEvent::Up(last));
                    return Some(TouchEvent::LongPress(origin));
                }

                // Release classification, queued behind the raw `Up`: a fired
                // LongPress suppresses the gesture; a net origin-to-last
                // delta of at least `swipe_min_distance` is a Swipe; an
                // unmoved touch is a Tap; a mid-range drag ends quietly.
                if !long_press_fired {
                    if chebyshev(last, origin) >= u32::from(self.swipe_min_distance) {
                        self.pending = Some(TouchEvent::Swipe(swipe_direction(origin, last)));
                    } else if !moved {
                        self.pending = Some(TouchEvent::Tap(origin));
                    }
                }

                Some(TouchEvent::Up(last))
            }
        }
    }

    /// Returns `true` while a touch is in progress (between the
    /// [`Down`](TouchEvent::Down) and [`Up`](TouchEvent::Up) edges).
    #[must_use]
    pub fn is_touched(&self) -> bool {
        matches!(self.state, State::Touched { .. })
    }

    /// Returns the current touch position: the last reported point (the Down
    /// point, re-anchored by each emitted [`Move`](TouchEvent::Move)) while
    /// touched, `None` when idle.
    ///
    /// This is the *rate-limited reported* position, not the raw live finger
    /// sample: sub-`move_epsilon` jitter after the last
    /// [`Move`](TouchEvent::Move) is not reflected, so it can lag the true
    /// finger position by up to `move_epsilon`.
    #[must_use]
    pub fn position(&self) -> Option<TouchPoint> {
        match self.state {
            State::Idle => None,
            State::Touched { last, .. } => Some(last),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // long_press = 600, swipe_min_distance = 50, move_epsilon = 10
    // (caller ticks / display pixels).
    fn tracker() -> TouchTracker {
        TouchTracker::new(600, 50, 10)
    }

    fn pt(x: i16, y: i16) -> TouchPoint {
        TouchPoint { x, y }
    }

    /// Frame-script helper: feeds `(touch, now)` frames and collects the
    /// emitted events.
    fn feed(tracker: &mut TouchTracker, frames: &[(Option<TouchPoint>, u64)]) -> Vec<TouchEvent> {
        let mut events = Vec::new();
        for &(touch, now) in frames {
            if let Some(event) = tracker.update(touch, now) {
                events.push(event);
            }
        }
        events
    }

    // --- Edges ---

    #[test]
    fn down_edge_emits_down_with_point() {
        let mut t = tracker();
        assert_eq!(
            t.update(Some(pt(120, 80)), 0),
            Some(TouchEvent::Down(pt(120, 80)))
        );
        assert!(t.is_touched());
    }

    #[test]
    fn up_edge_emits_up() {
        let mut t = tracker();
        t.update(Some(pt(120, 80)), 0);
        assert_eq!(t.update(None, 20), Some(TouchEvent::Up(pt(120, 80))));
        assert!(!t.is_touched());
    }

    #[test]
    fn second_touch_gets_fresh_origin_and_flags() {
        let mut t = tracker();
        // First touch: hold to LongPress, then drag — both flags set.
        t.update(Some(pt(0, 0)), 0);
        assert_eq!(
            t.update(Some(pt(0, 0)), 600),
            Some(TouchEvent::LongPress(pt(0, 0)))
        );
        assert_eq!(
            t.update(Some(pt(30, 0)), 620),
            Some(TouchEvent::Move(pt(30, 0)))
        );
        assert_eq!(t.update(None, 640), Some(TouchEvent::Up(pt(30, 0)))); // bare Up
        assert_eq!(t.update(None, 660), None);
        // Second touch: fresh origin, cleared flags — an unmoved quick tap
        // classifies as a Tap again.
        assert_eq!(
            t.update(Some(pt(100, 100)), 700),
            Some(TouchEvent::Down(pt(100, 100)))
        );
        assert_eq!(t.update(None, 720), Some(TouchEvent::Up(pt(100, 100))));
        assert_eq!(t.update(None, 740), Some(TouchEvent::Tap(pt(100, 100))));
    }

    #[test]
    fn every_down_bracketed_by_up() {
        // Mixed frame script (tap, swipe, long press): each Down is matched
        // by exactly one Up.
        let mut t = tracker();
        let frames: &[(Option<TouchPoint>, u64)] = &[
            // Quick tap.
            (Some(pt(10, 10)), 0),
            (None, 20), // Up
            (None, 40), // Tap drains
            // Swipe right.
            (Some(pt(0, 0)), 100),
            (Some(pt(60, 0)), 120), // Move
            (None, 140),            // Up
            (None, 160),            // Swipe drains
            // Long press.
            (Some(pt(50, 50)), 200),
            (Some(pt(50, 50)), 900), // LongPress
            (None, 920),             // bare Up
            (None, 940),
        ];
        let events = feed(&mut t, frames);
        let downs = events
            .iter()
            .filter(|e| matches!(e, TouchEvent::Down(_)))
            .count();
        let ups = events
            .iter()
            .filter(|e| matches!(e, TouchEvent::Up(_)))
            .count();
        assert_eq!(downs, 3);
        assert_eq!(downs, ups, "every Down must have a matching Up");
    }

    // --- Tap ---

    #[test]
    fn tap_sequence_down_up_then_tap_next_call() {
        let mut t = tracker();
        assert_eq!(
            t.update(Some(pt(120, 80)), 0),
            Some(TouchEvent::Down(pt(120, 80)))
        );
        assert_eq!(t.update(Some(pt(120, 80)), 20), None);
        assert_eq!(t.update(None, 40), Some(TouchEvent::Up(pt(120, 80))));
        // One event per call: the Tap drains on the next update.
        assert_eq!(t.update(None, 60), Some(TouchEvent::Tap(pt(120, 80))));
        assert_eq!(t.update(None, 80), None);
    }

    #[test]
    fn tap_at_exact_epsilon_still_tap() {
        let mut t = tracker();
        t.update(Some(pt(100, 100)), 0);
        // An excursion of exactly `move_epsilon` (Chebyshev) is "within":
        // no Move, no `moved` latch.
        assert_eq!(t.update(Some(pt(110, 100)), 20), None);
        assert_eq!(t.update(None, 40), Some(TouchEvent::Up(pt(100, 100))));
        assert_eq!(t.update(None, 60), Some(TouchEvent::Tap(pt(100, 100))));
    }

    #[test]
    fn move_past_epsilon_cancels_tap() {
        let mut t = tracker();
        t.update(Some(pt(100, 100)), 0);
        // One unit past epsilon latches `moved` and emits Move.
        assert_eq!(
            t.update(Some(pt(111, 100)), 20),
            Some(TouchEvent::Move(pt(111, 100)))
        );
        // Returning to the origin does not help: `moved` never resets, so
        // the release is a bare Up.
        assert_eq!(
            t.update(Some(pt(100, 100)), 40),
            Some(TouchEvent::Move(pt(100, 100)))
        );
        assert_eq!(t.update(None, 60), Some(TouchEvent::Up(pt(100, 100))));
        assert_eq!(t.update(None, 80), None); // no Tap
    }

    // --- Move ---

    #[test]
    fn move_emitted_and_reanchored() {
        let mut t = tracker();
        t.update(Some(pt(0, 0)), 0);
        assert_eq!(
            t.update(Some(pt(15, 0)), 20),
            Some(TouchEvent::Move(pt(15, 0)))
        );
        // Within epsilon of the *new* anchor at (15, 0) — nothing.
        assert_eq!(t.update(Some(pt(20, 0)), 40), None);
        // Past epsilon of the anchor — the next Move.
        assert_eq!(
            t.update(Some(pt(26, 0)), 60),
            Some(TouchEvent::Move(pt(26, 0)))
        );
    }

    #[test]
    fn jitter_within_epsilon_emits_nothing() {
        let mut t = tracker();
        t.update(Some(pt(100, 100)), 0);
        let mut now = 0_u64;
        for &(dx, dy) in &[(3_i16, -4_i16), (-5, 2), (0, 7), (-6, -6), (10, 10)] {
            now += 20;
            assert_eq!(t.update(Some(pt(100 + dx, 100 + dy)), now), None);
        }
        assert!(t.is_touched());
    }

    // --- LongPress ---

    #[test]
    fn long_press_fires_once_mid_hold() {
        let mut t = tracker();
        t.update(Some(pt(50, 50)), 0);
        assert_eq!(t.update(Some(pt(50, 50)), 300), None); // below threshold
        assert_eq!(
            t.update(Some(pt(50, 50)), 601),
            Some(TouchEvent::LongPress(pt(50, 50)))
        );
        assert_eq!(t.update(Some(pt(50, 50)), 700), None); // fires only once
    }

    #[test]
    fn long_press_exact_boundary() {
        let mut t = tracker();
        t.update(Some(pt(50, 50)), 0);
        assert_eq!(t.update(Some(pt(50, 50)), 599), None); // one tick short
                                                           // elapsed == long_press is inclusive.
        assert_eq!(
            t.update(Some(pt(50, 50)), 600),
            Some(TouchEvent::LongPress(pt(50, 50)))
        );
    }

    #[test]
    fn movement_cancels_long_press() {
        let mut t = tracker();
        t.update(Some(pt(50, 50)), 0);
        assert_eq!(
            t.update(Some(pt(70, 50)), 20),
            Some(TouchEvent::Move(pt(70, 50)))
        );
        // Far past the threshold: no LongPress — movement cancelled it
        // permanently.
        assert_eq!(t.update(Some(pt(70, 50)), 1000), None);
    }

    #[test]
    fn long_press_suppresses_tap() {
        let mut t = tracker();
        t.update(Some(pt(50, 50)), 0);
        assert_eq!(
            t.update(Some(pt(50, 50)), 600),
            Some(TouchEvent::LongPress(pt(50, 50)))
        );
        assert_eq!(t.update(None, 700), Some(TouchEvent::Up(pt(50, 50))));
        assert_eq!(t.update(None, 720), None); // no Tap
    }

    #[test]
    fn long_press_suppresses_swipe() {
        let mut t = tracker();
        t.update(Some(pt(0, 0)), 0);
        assert_eq!(
            t.update(Some(pt(0, 0)), 600),
            Some(TouchEvent::LongPress(pt(0, 0)))
        );
        // Drag past swipe_min_distance after the LongPress: Moves still flow…
        assert_eq!(
            t.update(Some(pt(60, 0)), 620),
            Some(TouchEvent::Move(pt(60, 0)))
        );
        // …but the release is a bare Up — no Swipe.
        assert_eq!(t.update(None, 640), Some(TouchEvent::Up(pt(60, 0))));
        assert_eq!(t.update(None, 660), None);
    }

    #[test]
    fn move_after_long_press_still_emitted() {
        let mut t = tracker();
        t.update(Some(pt(100, 100)), 0);
        assert_eq!(
            t.update(Some(pt(100, 100)), 600),
            Some(TouchEvent::LongPress(pt(100, 100)))
        );
        // Long-press-then-drag: Moves keep flowing after the LongPress.
        assert_eq!(
            t.update(Some(pt(120, 100)), 620),
            Some(TouchEvent::Move(pt(120, 100)))
        );
        assert_eq!(
            t.update(Some(pt(140, 100)), 640),
            Some(TouchEvent::Move(pt(140, 100)))
        );
    }

    #[test]
    fn coarse_poll_long_hold_emits_long_press_then_queued_up() {
        let mut t = tracker();
        t.update(Some(pt(50, 50)), 0);
        // The lift itself crosses the threshold with no mid-hold tick in
        // between: LongPress fires now, the Up is queued for the next call.
        assert_eq!(t.update(None, 900), Some(TouchEvent::LongPress(pt(50, 50))));
        assert_eq!(t.update(None, 920), Some(TouchEvent::Up(pt(50, 50))));
        assert_eq!(t.update(None, 940), None); // gesture suppressed — no Tap
    }

    // --- Swipe ---

    #[test]
    fn swipe_all_four_directions() {
        for (end, dir) in [
            (pt(0, -60), SwipeDirection::Up),
            (pt(0, 60), SwipeDirection::Down),
            (pt(-60, 0), SwipeDirection::Left),
            (pt(60, 0), SwipeDirection::Right),
        ] {
            let mut t = tracker();
            t.update(Some(pt(0, 0)), 0);
            assert_eq!(t.update(Some(end), 20), Some(TouchEvent::Move(end)));
            assert_eq!(t.update(None, 40), Some(TouchEvent::Up(end)));
            assert_eq!(t.update(None, 60), Some(TouchEvent::Swipe(dir)));
        }
    }

    #[test]
    fn swipe_exact_min_distance_fires() {
        let mut t = tracker();
        t.update(Some(pt(0, 0)), 0);
        assert_eq!(
            t.update(Some(pt(50, 0)), 20),
            Some(TouchEvent::Move(pt(50, 0)))
        );
        assert_eq!(t.update(None, 40), Some(TouchEvent::Up(pt(50, 0))));
        // A net delta of exactly `swipe_min_distance` is inclusive.
        assert_eq!(
            t.update(None, 60),
            Some(TouchEvent::Swipe(SwipeDirection::Right))
        );
    }

    #[test]
    fn one_unit_short_is_not_swipe() {
        let mut t = tracker();
        t.update(Some(pt(0, 0)), 0);
        assert_eq!(
            t.update(Some(pt(49, 0)), 20),
            Some(TouchEvent::Move(pt(49, 0)))
        );
        assert_eq!(t.update(None, 40), Some(TouchEvent::Up(pt(49, 0))));
        // 49 < 50: not a Swipe — and the drag also cancelled the Tap.
        assert_eq!(t.update(None, 60), None);
    }

    #[test]
    fn dominant_axis_selects_direction() {
        let mut t = tracker();
        t.update(Some(pt(0, 0)), 0);
        // dx = 60, dy = -80: |dy| > |dx| selects the vertical axis.
        t.update(Some(pt(60, -80)), 20); // Move
        t.update(None, 40); // Up
        assert_eq!(
            t.update(None, 60),
            Some(TouchEvent::Swipe(SwipeDirection::Up))
        );
    }

    #[test]
    fn axis_tie_goes_horizontal() {
        // |dx| == |dy|: the tie is documented to go horizontal.
        let mut t = tracker();
        t.update(Some(pt(0, 0)), 0);
        t.update(Some(pt(60, 60)), 20); // Move
        t.update(None, 40); // Up
        assert_eq!(
            t.update(None, 60),
            Some(TouchEvent::Swipe(SwipeDirection::Right))
        );

        let mut t = tracker();
        t.update(Some(pt(0, 0)), 0);
        t.update(Some(pt(-60, -60)), 20); // Move
        t.update(None, 40); // Up
        assert_eq!(
            t.update(None, 60),
            Some(TouchEvent::Swipe(SwipeDirection::Left))
        );
    }

    #[test]
    fn mid_range_drag_release_is_bare_up() {
        let mut t = tracker();
        t.update(Some(pt(0, 0)), 0);
        assert_eq!(
            t.update(Some(pt(30, 0)), 20),
            Some(TouchEvent::Move(pt(30, 0)))
        );
        assert_eq!(t.update(None, 40), Some(TouchEvent::Up(pt(30, 0))));
        // Past epsilon but short of swipe_min_distance: the drag ends
        // quietly — no Tap, no Swipe.
        assert_eq!(t.update(None, 60), None);
    }

    #[test]
    fn swipe_wins_when_config_inverted() {
        // swipe_min_distance (5) < move_epsilon (10) violates the documented
        // assumption but stays deterministic: once a Move re-anchors `last`,
        // the release classifies as a Swipe.
        let mut t = TouchTracker::new(600, 5, 10);
        t.update(Some(pt(0, 0)), 0);
        assert_eq!(
            t.update(Some(pt(12, 0)), 20),
            Some(TouchEvent::Move(pt(12, 0)))
        );
        assert_eq!(t.update(None, 40), Some(TouchEvent::Up(pt(12, 0))));
        assert_eq!(
            t.update(None, 60),
            Some(TouchEvent::Swipe(SwipeDirection::Right))
        );
    }

    #[test]
    fn diagonal_at_epsilon_stays_within() {
        // Chebyshev, not Euclidean: dx == dy == epsilon does not latch
        // `moved` (the Euclidean distance would exceed the epsilon).
        let mut t = tracker();
        t.update(Some(pt(100, 100)), 0);
        assert_eq!(t.update(Some(pt(110, 110)), 20), None);
        assert_eq!(t.update(None, 40), Some(TouchEvent::Up(pt(100, 100))));
        assert_eq!(t.update(None, 60), Some(TouchEvent::Tap(pt(100, 100))));
    }

    // --- Down/Move ordering ---

    #[test]
    fn no_move_on_down_call() {
        let mut t = tracker();
        // First contact emits only Down — never a Move, no matter where.
        assert_eq!(
            t.update(Some(pt(300, 200)), 0),
            Some(TouchEvent::Down(pt(300, 200)))
        );
        // A big jump right after the Down is a plain Move on the next call.
        assert_eq!(
            t.update(Some(pt(0, 0)), 20),
            Some(TouchEvent::Move(pt(0, 0)))
        );
    }

    #[test]
    fn one_frame_flicker_emits_up_and_fresh_origin_on_recontact() {
        // An undebounced one-frame `touched` flicker is a real lift: full Up
        // plus queued gesture, and re-contact anchors a fresh origin. The fix
        // for resistive panels is an upstream Debouncer (see module docs).
        let mut t = tracker();
        t.update(Some(pt(50, 50)), 0);
        assert_eq!(t.update(None, 20), Some(TouchEvent::Up(pt(50, 50))));
        // Re-contact on the drain tick: the queued Tap drains first…
        assert_eq!(
            t.update(Some(pt(52, 52)), 40),
            Some(TouchEvent::Tap(pt(50, 50)))
        );
        // …and the still-held touch anchors a fresh origin on the next call.
        assert_eq!(
            t.update(Some(pt(52, 52)), 60),
            Some(TouchEvent::Down(pt(52, 52)))
        );
    }

    // --- Robustness ---

    #[test]
    fn now_backwards_saturates_no_long_press() {
        let mut t = tracker();
        t.update(Some(pt(50, 50)), 1000);
        // A regressing clock clamps elapsed to zero — no LongPress, no panic.
        assert_eq!(t.update(Some(pt(50, 50)), 400), None);
        assert_eq!(t.update(Some(pt(50, 50)), 999), None);
        // Once the clock genuinely advances past the threshold, it fires.
        assert_eq!(
            t.update(Some(pt(50, 50)), 1600),
            Some(TouchEvent::LongPress(pt(50, 50)))
        );
    }

    #[test]
    fn near_u64_max_ticks_long_press_confirmed() {
        let mut t = tracker();
        t.update(Some(pt(50, 50)), u64::MAX - 600);
        assert_eq!(t.update(Some(pt(50, 50)), u64::MAX - 1), None); // 599 < 600
        assert_eq!(
            t.update(Some(pt(50, 50)), u64::MAX),
            Some(TouchEvent::LongPress(pt(50, 50)))
        );
    }

    #[test]
    fn extreme_i16_span_swipe_no_overflow() {
        // Full i16 span (-32768 → 32767): deltas are computed in i32, so
        // nothing overflows and the release is a clean Swipe.
        let mut t = tracker();
        t.update(Some(pt(i16::MIN, 0)), 0);
        assert_eq!(
            t.update(Some(pt(i16::MAX, 0)), 20),
            Some(TouchEvent::Move(pt(i16::MAX, 0)))
        );
        assert_eq!(t.update(None, 40), Some(TouchEvent::Up(pt(i16::MAX, 0))));
        assert_eq!(
            t.update(None, 60),
            Some(TouchEvent::Swipe(SwipeDirection::Right))
        );
    }

    #[test]
    fn down_on_drain_tick_is_delayed_not_lost() {
        let mut t = tracker();
        t.update(Some(pt(10, 10)), 0);
        assert_eq!(t.update(None, 20), Some(TouchEvent::Up(pt(10, 10)))); // pending = Tap
                                                                          // A touch landing exactly on the drain tick yields the queued Tap;
                                                                          // the touch frame itself is skipped and its coordinates are gone.
        assert_eq!(
            t.update(Some(pt(90, 90)), 40),
            Some(TouchEvent::Tap(pt(10, 10)))
        );
        assert!(!t.is_touched()); // the skipped frame did not change state
                                  // The still-held touch is picked up next call, anchored at the *next*
                                  // frame's point.
        assert_eq!(
            t.update(Some(pt(92, 92)), 60),
            Some(TouchEvent::Down(pt(92, 92)))
        );
    }

    #[test]
    fn is_touched_and_position_track_state() {
        let mut t = tracker();
        assert!(!t.is_touched());
        assert_eq!(t.position(), None);
        t.update(Some(pt(10, 20)), 0);
        assert!(t.is_touched());
        assert_eq!(t.position(), Some(pt(10, 20)));
        // `position` re-anchors on each emitted Move…
        t.update(Some(pt(40, 20)), 20); // Move
        assert_eq!(t.position(), Some(pt(40, 20)));
        // …but jitter within epsilon does not move the reported position.
        t.update(Some(pt(42, 20)), 40);
        assert_eq!(t.position(), Some(pt(40, 20)));
        t.update(None, 60); // Up
        assert!(!t.is_touched());
        assert_eq!(t.position(), None);
    }
}
