//! Semantic presence detection for digital sensors.
//!
//! A raw GPIO level often means "object present" or "object absent" rather
//! than "button pressed" or "button released".
//! This module names that domain directly with [`Presence`] and [`Polarity`],
//! then composes those with [`Debouncer`](crate::debounce::Debouncer) in
//! [`DigitalPresence`].
//! A `debounce` of `0` means no debouncing: the first changed sample
//! transitions immediately, matching [`Debouncer`](crate::debounce::Debouncer).
//! [`PresenceSession`] layers a warm-up-gated session state machine on top of
//! a `Presence` stream, counting sessions and measuring dwell while hiding the
//! boot pulses of latched sensors such as the HC-SR501 PIR module.
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

/// The lifecycle phase of a [`PresenceSession`].
///
/// A session always starts in [`WarmingUp`](SessionPhase::WarmingUp), moves
/// through [`Settling`](SessionPhase::Settling) exactly once, and stays in
/// [`Armed`](SessionPhase::Armed) for the rest of its life.
/// There is no way back to an earlier phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionPhase {
    /// Samples are being discarded except for spurious-pulse counting.
    ///
    /// This models a sensor's boot behavior — the HC-SR501 PIR module, for
    /// example, emits several seconds of unreliable output while its RC
    /// network settles.
    WarmingUp,
    /// Warm-up has ended but the session has not yet armed.
    ///
    /// The session waits here for the first observed
    /// [`Presence::Absent`] sample before arming, so a sensor that is still
    /// (falsely) reporting presence right at the warm-up deadline does not
    /// immediately produce a session.
    Settling,
    /// The session is live: every rising and falling edge produces a
    /// [`SessionEvent`].
    Armed,
}

/// Caller-declared metadata describing how a sensor's trigger behavior should
/// be read, with no effect on [`PresenceSession`] behavior.
///
/// Modeled on the HC-SR501 PIR module's H/L jumper.
/// The session state machine treats both variants identically; `TriggerMode`
/// exists purely so the caller can record and later display which physical
/// jumper setting the dwell numbers correspond to.
///
/// There is no [`Default`] impl: the caller must know which jumper setting is
/// on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerMode {
    /// H jumper: the sensor retriggers (extends the high output) while
    /// motion continues, so dwell tracks the observed high-output duration.
    Repeatable,
    /// L jumper: the sensor holds its output high for roughly a fixed
    /// delay-pot time regardless of continued motion, so dwell approximates
    /// that configured delay rather than the true motion duration.
    NonRepeatable,
}

/// An event emitted by [`PresenceSession::update`].
///
/// Exhaustive: no variant is hidden behind `#[non_exhaustive]`, so adding a
/// new variant is a breaking (0.x minor) change, not a silent addition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    /// A rising edge observed during
    /// [`WarmingUp`](SessionPhase::WarmingUp), counted in
    /// [`spurious_count`](PresenceSession::spurious_count) and otherwise
    /// discarded.
    WarmupPulse,
    /// A session began: a rising edge observed while
    /// [`Armed`](SessionPhase::Armed).
    ///
    /// `session` is 1-based and equals
    /// [`event_count`](PresenceSession::event_count) after the call that
    /// produced it.
    Started {
        /// The session count after this event, saturating at `u32::MAX`.
        session: u32,
    },
    /// A session ended: a falling edge observed while
    /// [`Armed`](SessionPhase::Armed).
    Ended {
        /// The session count this end pairs with, saturating at `u32::MAX`.
        session: u32,
        /// The observed signal duration between the accepted `Present`
        /// sample and this accepted `Absent` sample, in the caller's tick
        /// unit.
        dwell: u64,
    },
}

/// A warm-up-gated session state machine over a debounced [`Presence`]
/// stream.
///
/// Many latched presence sensors — the HC-SR501 PIR module is the reference
/// hardware — emit unreliable, spurious `Present` pulses for a warm-up
/// interval after power-up while an internal RC network settles.
/// `PresenceSession` hides that interval from the caller: it counts warm-up
/// pulses separately, waits for the sensor to report an honest `Absent`
/// sample before arming, and only then starts turning rising/falling edges
/// into [`Started`](SessionEvent::Started) / [`Ended`](SessionEvent::Ended)
/// session events with dwell measurement.
///
/// `PresenceSession` is a pure, HAL-agnostic consumer of [`Presence`]
/// values — it composes above [`DigitalPresence`] or
/// `DigitalPresenceInput` (`hal` feature), the same way [`button::ButtonDecoder`](crate::button::ButtonDecoder)
/// composes above [`debounce::EdgeDetector`](crate::debounce::EdgeDetector).
/// There is no trait and no `hal` adapter for `PresenceSession` itself: the
/// already-debounced `Presence` level is the seam.
///
/// # Lifecycle
///
/// A session moves through three phases, in order, with no way back:
///
/// 1. [`WarmingUp`](SessionPhase::WarmingUp) — from construction until `now`
///    reaches the warm-up deadline. Rising edges here are counted as
///    [`WarmupPulse`](SessionEvent::WarmupPulse) events; everything else is
///    silently discarded.
/// 2. [`Settling`](SessionPhase::Settling) — entered for exactly the sample
///    that first reaches the deadline. The session waits here, discarding
///    samples, until it observes [`Presence::Absent`], at which point it
///    arms.
/// 3. [`Armed`](SessionPhase::Armed) — every rising edge produces
///    [`Started`](SessionEvent::Started) and every falling edge produces
///    [`Ended`](SessionEvent::Ended) with the measured dwell.
///
/// # Example
///
/// ```
/// use tamer::presence::{
///     DigitalPresence, Polarity, Presence, PresenceSession, SessionEvent, TriggerMode,
/// };
///
/// let mut pir = DigitalPresence::new(false, Polarity::ActiveHigh, 0);
/// let mut session = PresenceSession::new(pir.stable_state(), 100, TriggerMode::Repeatable, 0);
///
/// // A spurious pulse during warm-up is counted but produces no session.
/// pir.update(true, 10);
/// assert_eq!(
///     session.update(pir.stable_state(), 10),
///     Some(SessionEvent::WarmupPulse)
/// );
/// pir.update(false, 20);
/// session.update(pir.stable_state(), 20);
///
/// // Warm-up ends with the sensor already absent, so the session arms silently.
/// pir.update(false, 100);
/// assert_eq!(session.update(pir.stable_state(), 100), None);
/// assert_eq!(session.spurious_count(), 1);
///
/// // Armed: every edge now produces a session event.
/// pir.update(true, 150);
/// assert_eq!(
///     session.update(pir.stable_state(), 150),
///     Some(SessionEvent::Started { session: 1 })
/// );
/// pir.update(false, 300);
/// assert_eq!(
///     session.update(pir.stable_state(), 300),
///     Some(SessionEvent::Ended { session: 1, dwell: 150 })
/// );
/// ```
///
/// # Zero warm-up still requires an `Absent` sample
///
/// `warmup == 0` removes the warm-up wait, but it does *not* arm the session:
/// arming always requires one [`Absent`](Presence::Absent) sample observed
/// through [`update`](PresenceSession::update).
/// A sensor already reporting [`Present`](Presence::Present) on the very first
/// poll is therefore suppressed as [`Settling`](SessionPhase::Settling) and
/// produces no session — the same rule that stops a sensor still falsely
/// reporting presence at the warm-up deadline from opening one.
/// This is the one contract most likely to surprise a caller, so it is worth
/// reading twice.
///
/// ```
/// use tamer::presence::{Presence, PresenceSession, SessionEvent, SessionPhase, TriggerMode};
///
/// let mut session = PresenceSession::new(Presence::Absent, 0, TriggerMode::Repeatable, 0);
///
/// // First sample is already `Present` — suppressed, *not* a `Started`.
/// assert_eq!(session.update(Presence::Present, 0), None);
/// assert_eq!(session.phase(), SessionPhase::Settling);
/// assert_eq!(session.event_count(), 0);
/// assert!(!session.is_active());
///
/// // Arming happens only once an `Absent` sample is observed.
/// assert_eq!(session.update(Presence::Absent, 10), None);
/// assert_eq!(session.phase(), SessionPhase::Armed);
///
/// // Only now does a rising edge open a session.
/// assert_eq!(
///     session.update(Presence::Present, 20),
///     Some(SessionEvent::Started { session: 1 })
/// );
/// assert!(session.is_active());
/// ```
///
/// # Composing with a hardware pin
///
/// With the `hal` feature, the same session composes above
/// `DigitalPresenceInput`, reading a real `embedded_hal::digital::InputPin`;
/// the example below uses `tamer::mock::MockInputPin` in its place.
/// Update the `DigitalPresenceInput` first, then feed its `stable_state()`
/// into `update` on every successful poll, including polls with no
/// transition.
///
/// ```
/// # #[cfg(feature = "hal")] {
/// use tamer::mock::MockInputPin;
/// use tamer::presence::{DigitalPresenceInput, Polarity, PresenceSession, SessionEvent, TriggerMode};
///
/// let pin = MockInputPin::new(false);
/// let mut pir = DigitalPresenceInput::new(pin, false, Polarity::ActiveHigh, 0);
/// let mut session = PresenceSession::new(pir.stable_state(), 0, TriggerMode::Repeatable, 0);
///
/// // warmup == 0: the first poll settles and arms on the observed `Absent`.
/// pir.update(0).unwrap();
/// assert_eq!(session.update(pir.stable_state(), 0), None);
///
/// pir.pin_mut().set_high();
/// pir.update(1).unwrap();
/// assert_eq!(
///     session.update(pir.stable_state(), 1),
///     Some(SessionEvent::Started { session: 1 })
/// );
///
/// pir.pin_mut().set_low();
/// pir.update(50).unwrap();
/// assert_eq!(
///     session.update(pir.stable_state(), 50),
///     Some(SessionEvent::Ended { session: 1, dwell: 49 })
/// );
/// # }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct PresenceSession {
    trigger_mode: TriggerMode,
    warmup_end: u64,
    phase: SessionPhase,
    level: Presence,
    events: u32,
    spurious: u32,
    last_onset: Option<u64>,
    last_dwell: Option<u64>,
    max_dwell: Option<u64>,
}

impl PresenceSession {
    /// Creates a new session, starting in
    /// [`WarmingUp`](SessionPhase::WarmingUp).
    ///
    /// `initial_level` seeds the stored level so a sensor already
    /// [`Present`](Presence::Present) at boot produces no rising edge on the
    /// first [`update`](PresenceSession::update) call.
    /// `warmup` is the warm-up duration in the caller's tick unit; the
    /// deadline is computed as `now.saturating_add(warmup)`, so it saturates
    /// at `u64::MAX` rather than wrapping — an intentionally-huge `warmup`
    /// simply keeps the session in
    /// [`WarmingUp`](SessionPhase::WarmingUp) until `now` itself reaches
    /// `u64::MAX`.
    /// `trigger_mode` is recorded verbatim for later reading; it has no
    /// effect on this session's behavior.
    #[must_use]
    pub fn new(initial_level: Presence, warmup: u64, trigger_mode: TriggerMode, now: u64) -> Self {
        Self {
            trigger_mode,
            warmup_end: now.saturating_add(warmup),
            phase: SessionPhase::WarmingUp,
            level: initial_level,
            events: 0,
            spurious: 0,
            last_onset: None,
            last_dwell: None,
            max_dwell: None,
        }
    }

    /// Feeds a debounced presence sample at the given timestamp.
    ///
    /// Call this on every successful poll of the upstream detector,
    /// including polls with no transition — `level` should be the detector's
    /// current [`stable_state`](DigitalPresence::stable_state), not only the
    /// samples where it changed.
    ///
    /// Behavior depends on the current [`phase`](PresenceSession::phase):
    ///
    /// - Only the sample that first reaches the warm-up deadline is
    ///   re-evaluated as [`Settling`](SessionPhase::Settling): a rising edge
    ///   observed on that exact sample is silent and *not* counted as
    ///   spurious, while the same rising edge one tick earlier is a counted
    ///   [`WarmupPulse`](SessionEvent::WarmupPulse).
    /// - Samples observed while [`Settling`](SessionPhase::Settling) are
    ///   dropped and never counted, spurious or otherwise.
    /// - Once [`Armed`](SessionPhase::Armed), every rising edge produces
    ///   [`Started`](SessionEvent::Started) and every falling edge produces
    ///   [`Ended`](SessionEvent::Ended) — boundary suppression never applies
    ///   to an armed session.
    /// - Construction never arms a session by itself: arming requires an
    ///   [`Absent`](Presence::Absent) sample observed through `update`. With
    ///   `warmup == 0`, the very first `Present` sample is suppressed as
    ///   [`Settling`](SessionPhase::Settling) rather than immediately
    ///   starting a session — this is intentional, not a bug.
    /// - [`TriggerMode`] has no behavioral effect here; it only records how
    ///   the caller should interpret the resulting dwell (H jumper /
    ///   [`Repeatable`](TriggerMode::Repeatable): presence retriggers while
    ///   motion continues; L jumper /
    ///   [`NonRepeatable`](TriggerMode::NonRepeatable): dwell approximates a
    ///   fixed delay-pot time).
    /// - Dwell is the observed signal duration between the accepted
    ///   `Present` sample and the accepted `Absent` sample — nothing more.
    /// - [`event_count`](PresenceSession::event_count) and
    ///   [`spurious_count`](PresenceSession::spurious_count) saturate at
    ///   `u32::MAX`; once saturated, `session` in
    ///   [`Started`](SessionEvent::Started) / [`Ended`](SessionEvent::Ended)
    ///   repeats `u32::MAX`, so it is a count, not a unique session id.
    /// - Clock regression is not detected: samples are processed normally,
    ///   `now.saturating_sub(onset)` clamps dwell to `0`, and no transition
    ///   depends on `now` decreasing, so phases never revert.
    /// - [`last_onset`](PresenceSession::last_onset) starts `None`, is set
    ///   on every [`Started`](SessionEvent::Started), and is retained
    ///   through the following [`Ended`](SessionEvent::Ended) until the next
    ///   [`Started`](SessionEvent::Started).
    ///   [`last_dwell`](PresenceSession::last_dwell) and
    ///   [`max_dwell`](PresenceSession::max_dwell) start `None` and change
    ///   only on [`Ended`](SessionEvent::Ended).
    /// - [`Armed`](SessionPhase::Armed) covers both idle and active
    ///   sessions; use [`level`](PresenceSession::level) to distinguish
    ///   them, or [`is_active`](PresenceSession::is_active) to combine both
    ///   checks into one call.
    /// - `warmup`, `now`, and `dwell` all share one caller-selected tick
    ///   unit; `PresenceSession` never assumes milliseconds.
    pub fn update(&mut self, level: Presence, now: u64) -> Option<SessionEvent> {
        let rising = self.level.is_absent() && level.is_present();
        let falling = self.level.is_present() && level.is_absent();
        self.level = level;

        if self.phase == SessionPhase::WarmingUp {
            if now < self.warmup_end {
                return if rising {
                    self.spurious = self.spurious.saturating_add(1);
                    Some(SessionEvent::WarmupPulse)
                } else {
                    None
                };
            }
            self.phase = SessionPhase::Settling;
        }

        if self.phase == SessionPhase::Settling {
            if level.is_absent() {
                self.phase = SessionPhase::Armed;
            }
            return None;
        }

        if rising {
            self.events = self.events.saturating_add(1);
            self.last_onset = Some(now);
            return Some(SessionEvent::Started {
                session: self.events,
            });
        }

        if falling {
            return self.last_onset.map(|onset| {
                let dwell = now.saturating_sub(onset);
                self.last_dwell = Some(dwell);
                self.max_dwell = Some(self.max_dwell.map_or(dwell, |max| max.max(dwell)));
                SessionEvent::Ended {
                    session: self.events,
                    dwell,
                }
            });
        }

        None
    }

    /// Returns the current lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> SessionPhase {
        self.phase
    }

    /// Returns the last presence level fed to
    /// [`update`](PresenceSession::update), or `initial_level` if `update`
    /// has never been called.
    #[must_use]
    pub const fn level(&self) -> Presence {
        self.level
    }

    /// Returns `true` if the session currently has an open window — a
    /// [`Started`](SessionEvent::Started) has fired and no matching
    /// [`Ended`](SessionEvent::Ended) has fired yet.
    ///
    /// Always `false` during [`WarmingUp`](SessionPhase::WarmingUp) and
    /// [`Settling`](SessionPhase::Settling), even if [`level`](Self::level) is
    /// [`Present`](Presence::Present) — presence observed before the session
    /// arms never produces a [`Started`](SessionEvent::Started) event.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self.phase, SessionPhase::Armed) && self.level.is_present()
    }

    /// Returns the number of sessions started while
    /// [`Armed`](SessionPhase::Armed), saturating at `u32::MAX`.
    #[must_use]
    pub const fn event_count(&self) -> u32 {
        self.events
    }

    /// Returns the number of rising edges observed while
    /// [`WarmingUp`](SessionPhase::WarmingUp), saturating at `u32::MAX`.
    #[must_use]
    pub const fn spurious_count(&self) -> u32 {
        self.spurious
    }

    /// Returns the timestamp of the most recent
    /// [`Started`](SessionEvent::Started) event, or `None` if no session has
    /// started yet.
    #[must_use]
    pub const fn last_onset(&self) -> Option<u64> {
        self.last_onset
    }

    /// Returns the dwell of the most recently completed session, or `None`
    /// if no session has ended yet.
    ///
    /// A dwell of `0` is a legitimate observed duration, which is why this
    /// returns `Option<u64>` rather than defaulting to `0`.
    #[must_use]
    pub const fn last_dwell(&self) -> Option<u64> {
        self.last_dwell
    }

    /// Returns the longest dwell observed across all completed sessions, or
    /// `None` if no session has ended yet.
    #[must_use]
    pub const fn max_dwell(&self) -> Option<u64> {
        self.max_dwell
    }

    /// Returns the caller-declared [`TriggerMode`], unchanged since
    /// construction.
    #[must_use]
    pub const fn trigger_mode(&self) -> TriggerMode {
        self.trigger_mode
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

    // --- PresenceSession ---

    /// Sample-script helper: feeds `(level, now)` samples and collects the
    /// emitted events.
    fn feed(session: &mut PresenceSession, samples: &[(Presence, u64)]) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        for &(level, now) in samples {
            if let Some(event) = session.update(level, now) {
                events.push(event);
            }
        }
        events
    }

    #[test]
    fn warmup_rising_edge_counts_pulse() {
        let mut session = PresenceSession::new(Presence::Absent, 100, TriggerMode::Repeatable, 0);

        assert_eq!(
            session.update(Presence::Present, 10),
            Some(SessionEvent::WarmupPulse)
        );
        assert_eq!(session.spurious_count(), 1);
        assert_eq!(session.event_count(), 0);
    }

    #[test]
    fn warmup_falling_edge_is_silent() {
        let mut session = PresenceSession::new(Presence::Present, 100, TriggerMode::Repeatable, 0);

        assert_eq!(session.update(Presence::Absent, 10), None);
        assert_eq!(session.spurious_count(), 0);
    }

    #[test]
    fn absent_at_exact_warmup_end_arms() {
        let mut session = PresenceSession::new(Presence::Present, 100, TriggerMode::Repeatable, 0);

        assert_eq!(session.update(Presence::Absent, 100), None);
        assert_eq!(session.phase(), SessionPhase::Armed);
        assert_eq!(session.event_count(), 0);
    }

    #[test]
    fn zero_warmup_first_present_is_settling() {
        let mut session = PresenceSession::new(Presence::Absent, 0, TriggerMode::Repeatable, 0);

        assert_eq!(session.update(Presence::Present, 0), None);
        assert_eq!(session.phase(), SessionPhase::Settling);
        assert_eq!(session.event_count(), 0);
    }

    #[test]
    fn zero_warmup_first_absent_arms() {
        let mut session = PresenceSession::new(Presence::Absent, 0, TriggerMode::Repeatable, 0);

        assert_eq!(session.update(Presence::Absent, 0), None);
        assert_eq!(session.phase(), SessionPhase::Armed);
    }

    #[test]
    fn rising_edge_at_warmup_end_is_silent_not_spurious() {
        let mut session = PresenceSession::new(Presence::Absent, 100, TriggerMode::Repeatable, 0);

        assert_eq!(session.update(Presence::Present, 100), None);
        assert_eq!(session.phase(), SessionPhase::Settling);
        assert_eq!(session.spurious_count(), 0);
    }

    #[test]
    fn rising_edge_one_tick_after_warmup_end_is_silent_not_spurious() {
        let mut session = PresenceSession::new(Presence::Absent, 100, TriggerMode::Repeatable, 0);

        assert_eq!(session.update(Presence::Present, 101), None);
        assert_eq!(session.phase(), SessionPhase::Settling);
        assert_eq!(session.spurious_count(), 0);
    }

    #[test]
    fn pulse_straddling_warmup_end_settles_then_arms_without_ended() {
        let mut session = PresenceSession::new(Presence::Absent, 100, TriggerMode::Repeatable, 0);

        assert_eq!(
            session.update(Presence::Present, 50),
            Some(SessionEvent::WarmupPulse)
        );
        assert_eq!(session.update(Presence::Present, 100), None);
        assert_eq!(session.phase(), SessionPhase::Settling);

        assert_eq!(session.update(Presence::Absent, 150), None);
        assert_eq!(session.phase(), SessionPhase::Armed);
        assert_eq!(session.event_count(), 0);
        assert_eq!(session.spurious_count(), 1);
    }

    #[test]
    fn armed_session_emits_started_then_ended_with_dwell() {
        let mut session = PresenceSession::new(Presence::Absent, 0, TriggerMode::Repeatable, 0);
        assert_eq!(session.update(Presence::Absent, 0), None); // settle -> arm

        assert_eq!(
            session.update(Presence::Present, 10),
            Some(SessionEvent::Started { session: 1 })
        );
        assert_eq!(
            session.update(Presence::Absent, 30),
            Some(SessionEvent::Ended {
                session: 1,
                dwell: 20
            })
        );
        assert_eq!(session.last_dwell(), Some(20));
        assert_eq!(session.max_dwell(), Some(20));
    }

    #[test]
    fn last_and_max_dwell_track_two_sessions() {
        let mut session = PresenceSession::new(Presence::Absent, 0, TriggerMode::Repeatable, 0);
        session.update(Presence::Absent, 0); // settle -> arm

        // First (longer) session: dwell 20.
        session.update(Presence::Present, 10);
        session.update(Presence::Absent, 30);
        assert_eq!(session.last_dwell(), Some(20));
        assert_eq!(session.max_dwell(), Some(20));

        // Second (shorter) session: dwell 5, does not raise max.
        session.update(Presence::Present, 40);
        session.update(Presence::Absent, 45);
        assert_eq!(session.last_dwell(), Some(5));
        assert_eq!(session.max_dwell(), Some(20));

        // Third (longest) session: dwell 50, raises max.
        session.update(Presence::Present, 50);
        session.update(Presence::Absent, 100);
        assert_eq!(session.last_dwell(), Some(50));
        assert_eq!(session.max_dwell(), Some(50));
    }

    #[test]
    fn last_dwell_holds_previous_value_during_active_session() {
        let mut session = PresenceSession::new(Presence::Absent, 0, TriggerMode::Repeatable, 0);
        session.update(Presence::Absent, 0); // settle -> arm

        session.update(Presence::Present, 10);
        session.update(Presence::Absent, 30);
        assert_eq!(session.last_dwell(), Some(20));

        // A new, still-open session must not clear last_dwell.
        session.update(Presence::Present, 40);
        assert_eq!(session.last_dwell(), Some(20));
    }

    #[test]
    fn last_onset_survives_ended() {
        let mut session = PresenceSession::new(Presence::Absent, 0, TriggerMode::Repeatable, 0);
        session.update(Presence::Absent, 0); // settle -> arm

        assert_eq!(session.last_onset(), None);
        session.update(Presence::Present, 10);
        assert_eq!(session.last_onset(), Some(10));
        session.update(Presence::Absent, 30);
        assert_eq!(session.last_onset(), Some(10));
    }

    #[test]
    fn saturated_deadline_stays_warming_up_below_u64_max() {
        let mut session = PresenceSession::new(
            Presence::Absent,
            100,
            TriggerMode::Repeatable,
            u64::MAX - 10,
        );

        assert_eq!(
            session.update(Presence::Present, u64::MAX - 1),
            Some(SessionEvent::WarmupPulse)
        );
        assert_eq!(session.phase(), SessionPhase::WarmingUp);
    }

    #[test]
    fn saturated_deadline_expires_at_u64_max() {
        let mut absent = PresenceSession::new(
            Presence::Absent,
            100,
            TriggerMode::Repeatable,
            u64::MAX - 10,
        );
        assert_eq!(absent.update(Presence::Absent, u64::MAX), None);
        assert_eq!(absent.phase(), SessionPhase::Armed);

        let mut present = PresenceSession::new(
            Presence::Absent,
            100,
            TriggerMode::Repeatable,
            u64::MAX - 10,
        );
        assert_eq!(present.update(Presence::Present, u64::MAX), None);
        assert_eq!(present.phase(), SessionPhase::Settling);
        assert_eq!(present.spurious_count(), 0);
    }

    #[test]
    fn absent_at_deadline_then_rise_one_tick_later_starts() {
        let mut session = PresenceSession::new(Presence::Absent, 100, TriggerMode::Repeatable, 0);

        assert_eq!(session.update(Presence::Absent, 100), None);
        assert_eq!(session.phase(), SessionPhase::Armed);
        assert_eq!(
            session.update(Presence::Present, 101),
            Some(SessionEvent::Started { session: 1 })
        );
        assert_eq!(session.spurious_count(), 0);
    }

    #[test]
    fn tick_regression_across_warmup_end_does_not_revert() {
        let mut session = PresenceSession::new(Presence::Absent, 100, TriggerMode::Repeatable, 0);

        assert_eq!(session.update(Presence::Absent, 150), None); // settle -> arm
        assert_eq!(session.phase(), SessionPhase::Armed);

        // now regresses well below the warm-up deadline; phase must not revert.
        assert_eq!(
            session.update(Presence::Present, 50),
            Some(SessionEvent::Started { session: 1 })
        );
        assert_eq!(session.phase(), SessionPhase::Armed);
    }

    #[test]
    fn tick_regression_in_open_session_gives_zero_dwell() {
        let mut session = PresenceSession::new(Presence::Absent, 0, TriggerMode::Repeatable, 0);
        session.update(Presence::Absent, 0); // settle -> arm

        assert_eq!(
            session.update(Presence::Present, 1000),
            Some(SessionEvent::Started { session: 1 })
        );
        assert_eq!(
            session.update(Presence::Absent, 500),
            Some(SessionEvent::Ended {
                session: 1,
                dwell: 0
            })
        );
        assert_eq!(session.phase(), SessionPhase::Armed);
    }

    #[test]
    fn same_sample_twice_is_noop() {
        let mut session = PresenceSession::new(Presence::Absent, 0, TriggerMode::Repeatable, 0);
        session.update(Presence::Absent, 0); // settle -> arm

        assert_eq!(
            session.update(Presence::Present, 10),
            Some(SessionEvent::Started { session: 1 })
        );
        assert_eq!(session.update(Presence::Present, 10), None);
        assert_eq!(session.event_count(), 1);
        assert_eq!(session.last_onset(), Some(10));
    }

    #[test]
    fn counters_saturate_and_event_reuses_u32_max() {
        // Looping to u32::MAX is infeasible; the test module shares privacy
        // with `PresenceSession`, so seed the counters directly.
        let mut session = PresenceSession::new(Presence::Absent, 0, TriggerMode::Repeatable, 0);
        assert_eq!(session.update(Presence::Absent, 0), None); // settle -> arm

        session.events = u32::MAX - 1;
        assert_eq!(
            session.update(Presence::Present, 1),
            Some(SessionEvent::Started { session: u32::MAX })
        );
        assert_eq!(session.event_count(), u32::MAX);
        assert_eq!(
            session.update(Presence::Absent, 2),
            Some(SessionEvent::Ended {
                session: u32::MAX,
                dwell: 1
            })
        );
        assert_eq!(
            session.update(Presence::Present, 3),
            Some(SessionEvent::Started { session: u32::MAX })
        );
        assert_eq!(session.event_count(), u32::MAX);

        let mut warming = PresenceSession::new(Presence::Absent, 1000, TriggerMode::Repeatable, 0);
        warming.spurious = u32::MAX - 1;
        assert_eq!(
            warming.update(Presence::Present, 1),
            Some(SessionEvent::WarmupPulse)
        );
        assert_eq!(warming.spurious_count(), u32::MAX);
        assert_eq!(warming.update(Presence::Absent, 2), None);
        assert_eq!(
            warming.update(Presence::Present, 3),
            Some(SessionEvent::WarmupPulse)
        );
        assert_eq!(warming.spurious_count(), u32::MAX);
    }

    #[test]
    fn initial_present_produces_no_rising_edge() {
        let mut session = PresenceSession::new(Presence::Present, 100, TriggerMode::Repeatable, 0);

        assert_eq!(session.update(Presence::Present, 10), None);
        assert_eq!(session.spurious_count(), 0);
    }

    #[test]
    fn is_active_tracks_open_session() {
        let mut session = PresenceSession::new(Presence::Absent, 100, TriggerMode::Repeatable, 0);

        // WarmingUp, even with a Present level fed in, is never active.
        assert_eq!(
            session.update(Presence::Present, 10),
            Some(SessionEvent::WarmupPulse)
        );
        assert!(!session.is_active());

        // Settling with a Present level is also never active.
        assert_eq!(session.update(Presence::Present, 100), None);
        assert_eq!(session.phase(), SessionPhase::Settling);
        assert!(!session.is_active());

        assert_eq!(session.update(Presence::Absent, 150), None);
        assert_eq!(session.phase(), SessionPhase::Armed);
        assert!(!session.is_active());

        assert_eq!(
            session.update(Presence::Present, 200),
            Some(SessionEvent::Started { session: 1 })
        );
        assert!(session.is_active());

        assert_eq!(
            session.update(Presence::Absent, 250),
            Some(SessionEvent::Ended {
                session: 1,
                dwell: 50
            })
        );
        assert!(!session.is_active());
    }

    #[test]
    fn trigger_modes_yield_identical_streams() {
        let samples = [
            (Presence::Present, 10u64),
            (Presence::Absent, 20),
            (Presence::Absent, 100),
            (Presence::Present, 110),
            (Presence::Absent, 140),
        ];

        let mut repeatable = PresenceSession::new(Presence::Absent, 50, TriggerMode::Repeatable, 0);
        let mut non_repeatable =
            PresenceSession::new(Presence::Absent, 50, TriggerMode::NonRepeatable, 0);

        let repeatable_events = feed(&mut repeatable, &samples);
        let non_repeatable_events = feed(&mut non_repeatable, &samples);

        assert_eq!(repeatable_events, non_repeatable_events);
        assert_eq!(repeatable.phase(), non_repeatable.phase());
        assert_eq!(repeatable.level(), non_repeatable.level());
        assert_eq!(repeatable.event_count(), non_repeatable.event_count());
        assert_eq!(repeatable.spurious_count(), non_repeatable.spurious_count());
        assert_eq!(repeatable.last_onset(), non_repeatable.last_onset());
        assert_eq!(repeatable.last_dwell(), non_repeatable.last_dwell());
        assert_eq!(repeatable.max_dwell(), non_repeatable.max_dwell());

        assert_eq!(repeatable.trigger_mode(), TriggerMode::Repeatable);
        assert_eq!(non_repeatable.trigger_mode(), TriggerMode::NonRepeatable);
    }

    #[test]
    fn armed_rising_edges_never_suppressed() {
        let mut session = PresenceSession::new(Presence::Absent, 0, TriggerMode::Repeatable, 0);
        session.update(Presence::Absent, 0); // settle -> arm

        for i in 1..=5u32 {
            let now = u64::from(i) * 10;
            assert_eq!(
                session.update(Presence::Present, now),
                Some(SessionEvent::Started { session: i })
            );
            assert_eq!(
                session.update(Presence::Absent, now + 1),
                Some(SessionEvent::Ended {
                    session: i,
                    dwell: 1
                })
            );
        }
    }

    #[cfg(feature = "hal")]
    #[test]
    fn composes_with_digital_presence_input() {
        use crate::mock::MockInputPin;

        let pin = MockInputPin::new(false);
        let mut pir = DigitalPresenceInput::new(pin, false, Polarity::ActiveHigh, 0);
        let mut session = PresenceSession::new(pir.stable_state(), 0, TriggerMode::Repeatable, 0);

        assert_eq!(pir.update(0).unwrap(), None);
        assert_eq!(session.update(pir.stable_state(), 0), None); // settle -> arm
        assert_eq!(session.phase(), SessionPhase::Armed);

        pir.pin_mut().set_high();
        assert_eq!(pir.update(1).unwrap(), Some(Presence::Present));
        assert_eq!(
            session.update(pir.stable_state(), 1),
            Some(SessionEvent::Started { session: 1 })
        );

        pir.pin_mut().set_low();
        assert_eq!(pir.update(50).unwrap(), Some(Presence::Absent));
        assert_eq!(
            session.update(pir.stable_state(), 50),
            Some(SessionEvent::Ended {
                session: 1,
                dwell: 49
            })
        );
    }
}
