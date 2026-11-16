//! Staged step timer — [`Stage`], [`StagedTimer`], and [`StagedEvent`].
//!
//! A [`StagedTimer`] walks a caller-owned, borrowed `&[Stage]` table.
//! Each [`Stage`] is a run of `count` steps, each lasting `interval_ticks`.
//! The timer only says **when**: it returns [`StagedEvent::Step`] each time a step is due and, in [`StagedMode::Once`], [`StagedEvent::Finished`] once the last step's interval has elapsed.
//! In [`StagedMode::Loop`] the table repeats forever and `Finished` is never emitted.
//! What a step *does* (advance a prop animation, blink, pick a value) is the caller's business.
//! A table such as `[6x70, 4x130, 3x220]` describes a slow-down: many quick steps, then fewer, slower ones.
//! Like the rest of `tamer`, this module is pure, HAL-agnostic, and alloc-free.
//!
//! # Clock
//!
//! The caller owns the clock, as with [`crate::tone`]: pass monotonic `u64` ticks (milliseconds, microseconds, raw timer counts) consistently to [`start`](StagedTimer::start) and [`update`](StagedTimer::update).
//!
//! # First step fires at start
//!
//! The first [`Step`](StagedEvent::Step) is due **at** the start tick, not one interval later.
//! This differs from [`crate::tone::ToneSequencer`], where note 0 plays implicitly from `start`.
//! The timeline is: step `k` is due at `start + (sum of the intervals of all earlier steps)`, and [`Finished`](StagedEvent::Finished) is due at `start + sum(count * interval_ticks)`.
//!
//! # Schedule accumulation
//!
//! Deadlines are absolute: each emitted step advances the deadline by that step's own interval and never rebases to the poll time.
//! A late poll never stretches the schedule and jitter does not accumulate.
//! A stalled caller catches up **one event per `update` call**.
//! In [`StagedMode::Once`] the `while let Some(ev) = timer.update(now)` idiom drains everything due and terminates because the table is finite.
//! In [`StagedMode::Loop`] it terminates only while the deadline stays above `now` after some step, which holds whenever the table has a non-zero interval and `now` is not near `u64::MAX`.
//! If every interval is `0`, or the deadline has saturated at `u64::MAX`, every call returns a step and the loop never ends, so bound it or poll once per tick instead.
//! Deadlines use [`saturating_add`](u64::saturating_add), and no `count * interval` product is ever computed.
//! The timer is not meant to span a tick-counter wrap; restart it after one.
//!
//! # Edge cases
//!
//! - A zero-count stage is skipped and adds no time.
//! - `interval_ticks == 0` is valid: one step per `update` call.
//! - An empty or all-zero-count table becomes active at `start` with [`Finished`](StagedEvent::Finished) due immediately, so the first `update` reports it exactly once.
//!   (`ToneSequencer` instead treats an empty table as already finished, without an event.)
//! - [`is_finished`](StagedTimer::is_finished) is `false` before `start` for every table.
//!
//! # Modes
//!
//! [`StagedMode::Once`] runs the table a single time and ends with [`Finished`](StagedEvent::Finished).
//! [`StagedMode::Loop`] wraps from the last step of the last non-empty stage to the first non-empty stage.
//! Deadlines keep accumulating absolutely from the start tick across the wrap, so a repeat pass has no drift.
//! A looping timer never emits `Finished` and [`is_finished`](StagedTimer::is_finished) stays `false`, provided the table has at least one step (`ToneSequencer` parity).
//! An empty or all-zero-count table behaves the same in both modes, reporting `Finished` once, so there is never a zero-time loop.
//!
//! # Which stage is running
//!
//! [`stage`](StagedTimer::stage) returns the table index of the stage the most recently emitted [`Step`](StagedEvent::Step) belongs to.
//! The index counts zero-count stages, so it is an index into the original slice.
//! It lets the caller treat one stage specially, such as a final one-step hold.
//!
//! # Example
//!
//! A prop animation that slows down: 6 quick steps, 4 medium, 3 slow, run once.
//! The first step fires at the start tick.
//! There are 13 steps in total, the last at tick 1380, and the slow-down ends at tick 1600.
//!
//! ```
//! use tamer::staged::{Stage, StagedEvent, StagedMode, StagedTimer};
//!
//! const SLOW_DOWN: [Stage; 3] = [
//!     Stage::new(6, 70),
//!     Stage::new(4, 130),
//!     Stage::new(3, 220),
//! ];
//!
//! let mut timer = StagedTimer::new(&SLOW_DOWN, StagedMode::Once);
//! timer.start(0);
//!
//! assert_eq!(timer.update(0), Some(StagedEvent::Step));
//! assert_eq!(timer.update(69), None);
//! assert_eq!(timer.update(70), Some(StagedEvent::Step));
//!
//! let mut steps = 2;
//! let mut now = 70;
//! while !timer.is_finished() {
//!     now += 10;
//!     match timer.update(now) {
//!         Some(StagedEvent::Step) => steps += 1,
//!         Some(StagedEvent::Finished) => assert_eq!(now, 1600),
//!         None => {}
//!     }
//! }
//! assert_eq!(steps, 13);
//! ```
//!
//! Stopping abandons the run without a `Finished` event, and a later `start` takes a fresh baseline:
//!
//! ```
//! use tamer::staged::{Stage, StagedEvent, StagedMode, StagedTimer};
//!
//! const TABLE: [Stage; 1] = [Stage::new(2, 10)];
//! let mut timer = StagedTimer::new(&TABLE, StagedMode::Once);
//!
//! timer.start(100);
//! assert_eq!(timer.update(100), Some(StagedEvent::Step));
//! timer.stop();
//! assert!(!timer.is_active());
//! assert_eq!(timer.update(1_000), None);
//!
//! timer.start(1_000);
//! assert_eq!(timer.update(1_000), Some(StagedEvent::Step));
//! ```
//!
//! An empty table reports `Finished` once:
//!
//! ```
//! use tamer::staged::{StagedEvent, StagedMode, StagedTimer};
//!
//! let mut timer = StagedTimer::new(&[], StagedMode::Once);
//! assert!(!timer.is_finished());
//! timer.start(7);
//! assert_eq!(timer.update(6), None);
//! assert_eq!(timer.update(7), Some(StagedEvent::Finished));
//! assert!(timer.is_finished());
//! assert_eq!(timer.update(8), None);
//! ```
//!
//! A dice prop that runs endlessly: tumble fast, slower, settle, then hold the result for 5 s and repeat.
//! The hold is just a final one-step stage, and [`stage`](StagedTimer::stage) tells the caller when it has arrived.
//!
//! ```
//! use tamer::staged::{Stage, StagedEvent, StagedMode, StagedTimer};
//!
//! const ROLL: [Stage; 4] = [
//!     Stage::new(6, 70),
//!     Stage::new(4, 130),
//!     Stage::new(3, 220),
//!     Stage::new(1, 5_000),
//! ];
//!
//! let mut timer = StagedTimer::new(&ROLL, StagedMode::Loop);
//! timer.start(0);
//!
//! let (mut faces, mut results) = (0, 0);
//! for now in 0..20_000 {
//!     match timer.update(now) {
//!         Some(StagedEvent::Step) if timer.stage() == Some(3) => results += 1, // show the result
//!         Some(StagedEvent::Step) => faces += 1, // show a random face
//!         Some(StagedEvent::Finished) => unreachable!("Loop never finishes"),
//!         None => {}
//!     }
//! }
//! assert!(!timer.is_finished());
//! // One pass is 6600 ticks: 13 tumble steps, then the hold step at tick 1600.
//! // Three full passes fit in 20 000 ticks, plus 3 tumble steps of the fourth.
//! assert_eq!((faces, results), (13 * 3 + 3, 3));
//! ```
//!
//! Catching up after a coarse poll, one event per call:
//!
//! ```
//! use tamer::staged::{Stage, StagedEvent, StagedMode, StagedTimer};
//!
//! const TABLE: [Stage; 1] = [Stage::new(3, 100)];
//! let mut timer = StagedTimer::new(&TABLE, StagedMode::Once);
//! timer.start(0);
//!
//! let mut steps = 0;
//! while let Some(ev) = timer.update(250) {
//!     assert_eq!(ev, StagedEvent::Step);
//!     steps += 1;
//! }
//! assert_eq!(steps, 3);
//! assert_eq!(timer.update(300), Some(StagedEvent::Finished));
//! ```

/// A run of `count` steps, each lasting `interval_ticks`.
///
/// Fields are `pub` so a table can be a `const` array literal.
/// Adding a field later is a breaking change, as with [`crate::tone::Note`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stage {
    /// Number of steps. `0` makes the stage a no-op that is skipped.
    pub count: u32,
    /// Duration of each step in caller-defined ticks.
    /// `0` is valid: steps advance one per [`update`](StagedTimer::update) call.
    pub interval_ticks: u64,
}

impl Stage {
    /// Creates a stage of `count` steps of `interval_ticks` each.
    #[must_use]
    pub const fn new(count: u32, interval_ticks: u64) -> Self {
        Self {
            count,
            interval_ticks,
        }
    }
}

/// An event returned by [`StagedTimer::update`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StagedEvent {
    /// One step is due now; the caller does its per-step work.
    Step,
    /// The last step's interval has elapsed.
    /// Fires exactly once per start in [`StagedMode::Once`], and also on the first
    /// update for an empty or all-zero-count table in either mode.
    /// Never fires in [`StagedMode::Loop`] for a table with at least one step.
    Finished,
}

/// How a [`StagedTimer`] treats the end of its table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StagedMode {
    /// Run the table once, then emit [`StagedEvent::Finished`].
    Once,
    /// Wrap to the first non-empty stage after the last step; never finishes
    /// (unless the table has no steps at all, which finishes once as in `Once`).
    Loop,
}

/// Pure staged step timer.
///
/// Borrows a `&'stages [Stage]` table and reports when each step is due.
/// Inactive until [`start`](Self::start); see the [module docs](self) for the timeline and edge-case rules.
/// Copying a timer copies its progress, as with `ToneSequencer`.
///
/// # Example
///
/// ```
/// use tamer::staged::{Stage, StagedMode, StagedTimer};
///
/// const TABLE: [Stage; 1] = [Stage::new(1, 5)];
/// let mut timer = StagedTimer::new(&TABLE, StagedMode::Once);
/// assert!(!timer.is_active());
/// timer.start(0);
/// assert!(timer.is_active());
/// ```
#[derive(Debug, Clone, Copy)]
pub struct StagedTimer<'stages> {
    stages: &'stages [Stage],
    mode: StagedMode,
    active: bool,
    finished: bool,
    index: usize,
    left: u32,
    due: u64,
    draining: bool,
    last_stage: Option<usize>,
}

impl<'stages> StagedTimer<'stages> {
    /// Creates an inactive timer over the given stage table.
    ///
    /// `mode` selects [`StagedMode::Once`] or [`StagedMode::Loop`] behaviour at the end of the table.
    ///
    /// [`is_finished`](Self::is_finished) is `false` until
    /// [`start`](Self::start), even for an empty table.
    #[must_use]
    pub const fn new(stages: &'stages [Stage], mode: StagedMode) -> Self {
        Self {
            stages,
            mode,
            active: false,
            finished: false,
            index: 0,
            left: 0,
            due: 0,
            draining: false,
            last_stage: None,
        }
    }

    /// Index of the first stage at or after `from` with a non-zero count.
    const fn next_nonempty(&self, mut from: usize) -> Option<usize> {
        while from < self.stages.len() {
            if self.stages[from].count > 0 {
                return Some(from);
            }
            from += 1;
        }
        None
    }

    /// (Re)starts the timer: the first step is due at `now`.
    ///
    /// Leading zero-count stages are skipped, `finished` is cleared, and [`stage`](Self::stage) resets to `None`.
    /// If no stage has steps, the timer becomes active with only [`Finished`](StagedEvent::Finished) pending at `now`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tamer::staged::{Stage, StagedEvent, StagedMode, StagedTimer};
    ///
    /// const TABLE: [Stage; 1] = [Stage::new(2, 10)];
    /// let mut timer = StagedTimer::new(&TABLE, StagedMode::Once);
    /// timer.start(100);
    /// assert!(timer.is_active());
    /// assert_eq!(timer.update(99), None);
    /// assert_eq!(timer.update(100), Some(StagedEvent::Step));
    /// ```
    pub fn start(&mut self, now: u64) {
        self.active = true;
        self.finished = false;
        self.last_stage = None;
        self.due = now;
        match self.next_nonempty(0) {
            Some(i) => {
                self.index = i;
                self.left = self.stages[i].count;
                self.draining = false;
            }
            None => {
                self.index = 0;
                self.left = 0;
                self.draining = true;
            }
        }
    }

    /// Abandons the run without a [`Finished`](StagedEvent::Finished) event.
    ///
    /// Clears `active` only; `finished` is untouched (as `ToneSequencer`).
    pub fn stop(&mut self) {
        self.active = false;
    }

    /// Advances the timer to `now` and returns at most one event.
    ///
    /// In [`StagedMode::Loop`] this never returns [`Finished`](StagedEvent::Finished) for a table with at least one step.
    ///
    /// Returns `None` while inactive or while `now` is before the next deadline (inclusive boundary: `now >= due` fires).
    /// A `now` that moves backwards is not an error: it returns `None` only while it is still before the next deadline, and fires as usual once it is at or past it.
    ///
    /// # Examples
    ///
    /// ```
    /// use tamer::staged::{Stage, StagedEvent, StagedMode, StagedTimer};
    ///
    /// const TABLE: [Stage; 1] = [Stage::new(1, 10)];
    /// let mut timer = StagedTimer::new(&TABLE, StagedMode::Once);
    /// timer.start(0);
    /// assert_eq!(timer.update(0), Some(StagedEvent::Step));
    /// assert_eq!(timer.update(9), None);
    /// assert_eq!(timer.update(10), Some(StagedEvent::Finished));
    /// ```
    #[must_use]
    pub fn update(&mut self, now: u64) -> Option<StagedEvent> {
        if !self.active || now < self.due {
            return None;
        }
        if self.draining {
            self.active = false;
            self.finished = true;
            return Some(StagedEvent::Finished);
        }
        let Some(stage) = self.stages.get(self.index) else {
            // Defensive: unreachable for a table that is consistent with `start`.
            self.active = false;
            return None;
        };
        self.due = self.due.saturating_add(stage.interval_ticks);
        self.left = self.left.saturating_sub(1);
        self.last_stage = Some(self.index);
        if self.left == 0 {
            let next = match (self.next_nonempty(self.index.saturating_add(1)), self.mode) {
                (None, StagedMode::Loop) => self.next_nonempty(0),
                (found, _) => found,
            };
            match next {
                Some(i) => {
                    self.index = i;
                    self.left = self.stages[i].count;
                }
                None => self.draining = true,
            }
        }
        Some(StagedEvent::Step)
    }

    /// Returns `true` between [`start`](Self::start) and [`Finished`](StagedEvent::Finished) or [`stop`](Self::stop).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    /// Returns `true` once [`Finished`](StagedEvent::Finished) has fired, until the next [`start`](Self::start).
    ///
    /// Stays `false` in [`StagedMode::Loop`] for a table with at least one step.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Table index of the stage the most recently emitted [`Step`](StagedEvent::Step) belongs to.
    ///
    /// The index is into the original slice and counts zero-count stages.
    /// It is `None` after [`new`](Self::new) and after [`start`](Self::start) until the first step.
    /// It is **not** cleared by [`stop`](Self::stop) or [`Finished`](StagedEvent::Finished).
    ///
    /// # Examples
    ///
    /// ```
    /// use tamer::staged::{Stage, StagedMode, StagedTimer};
    ///
    /// const TABLE: [Stage; 3] = [Stage::new(0, 5), Stage::new(1, 10), Stage::new(1, 10)];
    /// let mut timer = StagedTimer::new(&TABLE, StagedMode::Loop);
    /// assert_eq!(timer.stage(), None);
    /// timer.start(0);
    /// assert_eq!(timer.stage(), None);
    /// let _ = timer.update(0);
    /// assert_eq!(timer.stage(), Some(1));
    /// let _ = timer.update(10);
    /// assert_eq!(timer.stage(), Some(2));
    /// ```
    #[must_use]
    pub const fn stage(&self) -> Option<usize> {
        self.last_stage
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONCE: StagedMode = StagedMode::Once;
    const LOOP: StagedMode = StagedMode::Loop;

    const THREE_STAGE: [Stage; 3] = [Stage::new(6, 70), Stage::new(4, 130), Stage::new(3, 220)];
    const STEP_TICKS: [u64; 13] = [
        0, 70, 140, 210, 280, 350, 420, 550, 680, 810, 940, 1160, 1380,
    ];

    /// Drains all events due at `now`, bounded.
    fn drain(t: &mut StagedTimer<'_>, now: u64) -> (usize, usize) {
        let (mut s, mut f) = (0, 0);
        for _ in 0..100 {
            match t.update(now) {
                Some(StagedEvent::Step) => s += 1,
                Some(StagedEvent::Finished) => f += 1,
                None => break,
            }
        }
        (s, f)
    }

    // --- construction ---

    #[test]
    fn stage_new_stores_fields_verbatim() {
        let s = Stage::new(7, 99);
        assert_eq!(s.count, 7);
        assert_eq!(s.interval_ticks, 99);
    }

    #[test]
    fn const_construction() {
        const TABLE: [Stage; 1] = [Stage::new(1, 1)];
        const T: StagedTimer<'static> = StagedTimer::new(&TABLE, ONCE);
        assert!(!T.is_active());
    }

    #[test]
    fn inactive_before_start() {
        for table in [&THREE_STAGE[..], &[]] {
            let mut t = StagedTimer::new(table, ONCE);
            assert!(!t.is_active());
            assert!(!t.is_finished());
            assert_eq!(t.update(0), None);
            assert_eq!(t.update(u64::MAX), None);
            assert!(!t.is_finished());
        }
    }

    // --- timeline ---

    #[test]
    fn exact_timeline() {
        let mut t = StagedTimer::new(&THREE_STAGE, ONCE);
        t.start(0);
        assert!(t.is_active());
        for &tick in &STEP_TICKS {
            if tick > 0 {
                assert_eq!(t.update(tick - 1), None, "before {tick}");
            }
            assert_eq!(t.update(tick), Some(StagedEvent::Step), "at {tick}");
        }
        assert_eq!(t.update(1599), None);
        assert!(!t.is_finished());
        assert_eq!(t.update(1600), Some(StagedEvent::Finished));
        assert!(t.is_finished());
        assert!(!t.is_active());
        assert_eq!(t.update(1600), None);
        assert_eq!(t.update(u64::MAX), None);
    }

    #[test]
    fn first_step_fires_at_start_tick() {
        let mut t = StagedTimer::new(&THREE_STAGE, ONCE);
        t.start(500);
        assert_eq!(t.update(499), None);
        assert_eq!(t.update(500), Some(StagedEvent::Step));
        assert_eq!(t.update(500), None);
    }

    #[test]
    fn boundary_off_by_one() {
        let table = [Stage::new(1, 10)];
        let mut t = StagedTimer::new(&table, ONCE);
        t.start(0);
        assert_eq!(t.update(0), Some(StagedEvent::Step));
        assert_eq!(t.update(9), None);
        assert_eq!(t.update(10), Some(StagedEvent::Finished));
    }

    // --- catch-up and drift ---

    #[test]
    fn coarse_poll_catches_up_one_per_call_without_drift() {
        let table = [Stage::new(3, 100)];
        let mut t = StagedTimer::new(&table, ONCE);
        t.start(0);
        assert_eq!(t.update(250), Some(StagedEvent::Step));
        assert_eq!(t.update(250), Some(StagedEvent::Step));
        assert_eq!(t.update(250), Some(StagedEvent::Step));
        assert_eq!(t.update(250), None);
        assert_eq!(t.update(300), Some(StagedEvent::Finished));
    }

    #[test]
    fn jitter_does_not_accumulate() {
        let table = [Stage::new(3, 100)];
        let mut t = StagedTimer::new(&table, ONCE);
        t.start(0);
        assert_eq!(t.update(0), Some(StagedEvent::Step));
        assert_eq!(t.update(105), Some(StagedEvent::Step));
        assert_eq!(t.update(199), None);
        assert_eq!(t.update(200), Some(StagedEvent::Step));
    }

    #[test]
    fn late_poll_does_not_stretch_schedule() {
        let mut t = StagedTimer::new(&THREE_STAGE, ONCE);
        t.start(0);
        assert_eq!(drain(&mut t, 1_000_000), (13, 1));
        assert_eq!(t.update(1_000_000), None);
    }

    // --- zero interval / zero count ---

    #[test]
    fn zero_interval_one_event_per_call() {
        let table = [Stage::new(2, 0), Stage::new(1, 10)];
        let mut t = StagedTimer::new(&table, ONCE);
        t.start(0);
        assert_eq!(t.update(0), Some(StagedEvent::Step));
        assert_eq!(t.update(0), Some(StagedEvent::Step));
        assert_eq!(t.update(0), Some(StagedEvent::Step));
        assert_eq!(t.update(0), None);
        assert_eq!(t.update(10), Some(StagedEvent::Finished));

        let only = [Stage::new(2, 0)];
        let mut t = StagedTimer::new(&only, ONCE);
        t.start(0);
        assert_eq!(t.update(0), Some(StagedEvent::Step));
        assert_eq!(t.update(0), Some(StagedEvent::Step));
        assert_eq!(t.update(0), Some(StagedEvent::Finished));
        assert_eq!(t.update(0), None);
    }

    #[test]
    fn zero_count_stages_skipped() {
        let table = [
            Stage::new(0, 50),
            Stage::new(2, 10),
            Stage::new(0, 99),
            Stage::new(1, 5),
            Stage::new(0, 7),
        ];
        let mut t = StagedTimer::new(&table, ONCE);
        t.start(0);
        for tick in [0, 10, 20] {
            assert_eq!(t.update(tick), Some(StagedEvent::Step), "at {tick}");
        }
        assert_eq!(t.update(24), None);
        assert_eq!(t.update(25), Some(StagedEvent::Finished));
    }

    #[test]
    fn leading_run_of_zero_count_stages() {
        let table = [Stage::new(0, 1), Stage::new(0, 2), Stage::new(3, 10)];
        let mut t = StagedTimer::new(&table, ONCE);
        t.start(0);
        assert_eq!(drain(&mut t, 1_000), (3, 1));
    }

    #[test]
    fn empty_and_all_zero_table_emit_finished_once() {
        let zeros = [Stage::new(0, 5), Stage::new(0, 9)];
        for table in [&[][..], &zeros[..]] {
            let mut t = StagedTimer::new(table, ONCE);
            t.start(7);
            assert!(t.is_active());
            assert_eq!(t.update(6), None);
            assert_eq!(t.update(7), Some(StagedEvent::Finished));
            assert!(t.is_finished());
            assert!(!t.is_active());
            assert_eq!(t.update(8), None);
        }
    }

    // --- start / stop ---

    #[test]
    fn start_restarts_with_fresh_baseline() {
        let mut t = StagedTimer::new(&THREE_STAGE, ONCE);
        t.start(0);
        assert_eq!(t.update(0), Some(StagedEvent::Step));
        assert_eq!(t.update(70), Some(StagedEvent::Step));
        t.start(1000);
        assert_eq!(t.update(999), None);
        assert_eq!(t.update(1000), Some(StagedEvent::Step));
        assert_eq!(t.update(1070), Some(StagedEvent::Step));

        // After Finished: finished cleared, full timeline replays.
        let mut t = StagedTimer::new(&THREE_STAGE, ONCE);
        t.start(0);
        assert_eq!(drain(&mut t, 5_000), (13, 1));
        assert!(t.is_finished());
        t.start(10_000);
        assert!(!t.is_finished());
        assert!(t.is_active());
        assert_eq!(drain(&mut t, 20_000), (13, 1));
    }

    #[test]
    fn stop_abandons_without_finished() {
        let mut t = StagedTimer::new(&THREE_STAGE, ONCE);
        t.start(0);
        assert_eq!(t.update(0), Some(StagedEvent::Step));
        t.stop();
        assert!(!t.is_active());
        assert_eq!(t.update(u64::MAX), None);
        assert!(!t.is_finished());

        t.start(100);
        assert_eq!(t.update(100), Some(StagedEvent::Step));

        // Stop after Finished keeps finished.
        let mut t = StagedTimer::new(&THREE_STAGE, ONCE);
        t.start(0);
        let _ = drain(&mut t, 5_000);
        t.stop();
        assert!(t.is_finished());
        t.start(0);
        assert!(!t.is_finished());
    }

    #[test]
    fn backward_now_before_deadline_returns_none() {
        let mut t = StagedTimer::new(&THREE_STAGE, ONCE);
        t.start(100);
        assert_eq!(t.update(50), None);
        assert_eq!(t.update(100), Some(StagedEvent::Step));
        assert_eq!(t.update(90), None);
        assert_eq!(t.update(169), None);
        assert_eq!(t.update(170), Some(StagedEvent::Step));
    }

    #[test]
    fn backward_now_past_deadline_still_fires() {
        let mut t = StagedTimer::new(&THREE_STAGE, ONCE);
        t.start(0);
        assert_eq!(t.update(1_000), Some(StagedEvent::Step));
        assert_eq!(t.update(500), Some(StagedEvent::Step));
        assert_eq!(t.update(150), Some(StagedEvent::Step));
        assert_eq!(t.update(140), None);
        assert_eq!(t.update(210), Some(StagedEvent::Step));
    }

    // --- saturation ---

    #[test]
    fn near_u64_max() {
        let table = [Stage::new(2, 10)];
        let mut t = StagedTimer::new(&table, ONCE);
        t.start(u64::MAX - 10);
        assert_eq!(t.update(u64::MAX - 10), Some(StagedEvent::Step));
        assert_eq!(t.update(u64::MAX - 1), None);
        assert_eq!(t.update(u64::MAX), Some(StagedEvent::Step));
        assert_eq!(t.update(u64::MAX), Some(StagedEvent::Finished));
        assert_eq!(t.update(u64::MAX), None);
    }

    #[test]
    fn saturating_interval_terminates() {
        let table = [Stage::new(5, u64::MAX)];
        let mut t = StagedTimer::new(&table, ONCE);
        t.start(u64::MAX);
        let (s, f) = drain(&mut t, u64::MAX);
        assert_eq!((s, f), (5, 1));
    }

    // --- loop mode ---

    const DICE: [Stage; 4] = [
        Stage::new(6, 70),
        Stage::new(4, 130),
        Stage::new(3, 220),
        Stage::new(1, 5_000),
    ];

    /// One `DICE` pass: 13 tumble steps (last at 1380), then the hold step at 1600 lasting 5000.
    const DICE_PASS: u64 = 1600 + 5_000;

    #[test]
    fn loop_wraps_with_exact_timeline() {
        let mut t = StagedTimer::new(&DICE, LOOP);
        t.start(0);
        for pass in 0..3u64 {
            let base = pass * DICE_PASS;
            let mut ticks = [0u64; 14];
            ticks[..13].copy_from_slice(&STEP_TICKS);
            ticks[13] = 1600;
            for &tick in &ticks {
                let at = base + tick;
                if at > 0 {
                    assert_eq!(t.update(at - 1), None, "before {at}");
                }
                assert_eq!(t.update(at), Some(StagedEvent::Step), "at {at}");
            }
            assert_eq!(t.update(base + DICE_PASS - 1), None);
        }
        assert!(!t.is_finished());
        assert!(t.is_active());
    }

    #[test]
    fn loop_never_finishes() {
        let mut t = StagedTimer::new(&DICE, LOOP);
        t.start(0);
        for now in (0..100_000).step_by(7) {
            assert_ne!(t.update(now), Some(StagedEvent::Finished));
            assert!(!t.is_finished());
            assert!(t.is_active());
        }
    }

    #[test]
    fn stage_values_across_pass_and_wrap() {
        let mut t = StagedTimer::new(&DICE, LOOP);
        assert_eq!(t.stage(), None);
        t.start(0);
        assert_eq!(t.stage(), None);
        let expected = [0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 3];
        for _pass in 0..2 {
            for &e in &expected {
                assert_eq!(t.update(u64::MAX / 2), Some(StagedEvent::Step));
                assert_eq!(t.stage(), Some(e));
            }
        }
    }

    #[test]
    fn stage_skips_zero_count_stages() {
        let table = [
            Stage::new(0, 9),
            Stage::new(1, 10),
            Stage::new(0, 9),
            Stage::new(2, 10),
            Stage::new(0, 9),
        ];
        let mut t = StagedTimer::new(&table, LOOP);
        t.start(0);
        let mut seen = [None; 7];
        for (i, s) in seen.iter_mut().enumerate() {
            assert_eq!(t.update(i as u64 * 10), Some(StagedEvent::Step));
            *s = t.stage();
        }
        assert_eq!(
            seen,
            [
                Some(1),
                Some(3),
                Some(3),
                Some(1),
                Some(3),
                Some(3),
                Some(1)
            ]
        );
    }

    #[test]
    fn stage_none_before_first_step_and_reset_by_restart() {
        let mut t = StagedTimer::new(&THREE_STAGE, ONCE);
        assert_eq!(t.stage(), None);
        t.start(100);
        assert_eq!(t.stage(), None);
        assert_eq!(t.update(99), None);
        assert_eq!(t.stage(), None);
        assert_eq!(t.update(100), Some(StagedEvent::Step));
        assert_eq!(t.stage(), Some(0));
        t.start(500);
        assert_eq!(t.stage(), None);
        assert_eq!(t.update(500), Some(StagedEvent::Step));
        assert_eq!(t.stage(), Some(0));
    }

    #[test]
    fn stage_survives_stop_and_finished() {
        let mut t = StagedTimer::new(&THREE_STAGE, ONCE);
        t.start(0);
        assert_eq!(drain(&mut t, 5_000), (13, 1));
        assert_eq!(t.stage(), Some(2));
        t.stop();
        assert_eq!(t.stage(), Some(2));

        let mut t = StagedTimer::new(&THREE_STAGE, LOOP);
        t.start(0);
        assert_eq!(t.update(0), Some(StagedEvent::Step));
        t.stop();
        assert_eq!(t.stage(), Some(0));
    }

    #[test]
    fn loop_empty_and_all_zero_table_emit_finished_once() {
        let zeros = [Stage::new(0, 5), Stage::new(0, 9)];
        for table in [&[][..], &zeros[..]] {
            let mut t = StagedTimer::new(table, LOOP);
            t.start(7);
            assert_eq!(t.update(6), None);
            assert_eq!(t.update(7), Some(StagedEvent::Finished));
            assert!(t.is_finished());
            assert!(!t.is_active());
            assert_eq!(t.update(8), None);
            assert_eq!(t.stage(), None);
        }
    }

    #[test]
    fn loop_late_poll_catches_up_across_wrap_without_drift() {
        let mut t = StagedTimer::new(&DICE, LOOP);
        t.start(0);
        // Steps due at or before 2 passes + 100: 14 + 14 + 2 (ticks 0 and 70 of pass 3).
        let now = 2 * DICE_PASS + 100;
        let mut n = 0;
        while n < 100 && t.update(now).is_some() {
            n += 1;
        }
        assert_eq!(n, 30);
        // Pass 3's next step is exactly on schedule at tick 140.
        assert_eq!(t.update(2 * DICE_PASS + 139), None);
        assert_eq!(t.update(2 * DICE_PASS + 140), Some(StagedEvent::Step));
    }

    #[test]
    fn loop_single_stage_repeats() {
        let table = [Stage::new(1, 10)];
        let mut t = StagedTimer::new(&table, LOOP);
        t.start(0);
        for k in 0..5u64 {
            assert_eq!(t.update(k * 10), Some(StagedEvent::Step));
            assert_eq!(t.stage(), Some(0));
            assert_eq!(t.update(k * 10), None);
        }
    }

    #[test]
    fn loop_near_u64_max_does_not_panic() {
        let table = [Stage::new(2, 10), Stage::new(1, 10)];
        let mut t = StagedTimer::new(&table, LOOP);
        t.start(u64::MAX - 15);
        for _ in 0..50 {
            assert_eq!(t.update(u64::MAX), Some(StagedEvent::Step));
        }
        assert!(!t.is_finished());
        assert!(t.stage().is_some());
    }

    #[test]
    fn loop_zero_interval_one_event_per_call() {
        let table = [Stage::new(2, 0)];
        let mut t = StagedTimer::new(&table, LOOP);
        t.start(0);
        for _ in 0..10 {
            assert_eq!(t.update(0), Some(StagedEvent::Step));
        }
        assert!(!t.is_finished());
    }

    #[test]
    fn huge_count_emits_without_overflow() {
        let table = [Stage::new(u32::MAX, 1)];
        let mut t = StagedTimer::new(&table, ONCE);
        t.start(0);
        assert_eq!(t.update(0), Some(StagedEvent::Step));
        assert_eq!(t.update(1), Some(StagedEvent::Step));
        assert_eq!(t.update(1), None);
        assert_eq!(t.update(2), Some(StagedEvent::Step));
    }
}
