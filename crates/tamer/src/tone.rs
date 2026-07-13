//! Tone/duration sequencer — [`Note`], [`ToneSequencer`], [`SequenceMode`],
//! [`ToneOutput`], and [`SequenceEvent`].
//!
//! A [`ToneSequencer`] steps through a caller-owned, borrowed `&[Note]` table —
//! a "melody" of frequency + duration + amplitude steps, played once or looped
//! — and reports the current frequency/amplitude for a downstream driver to
//! push to a buzzer (PWM square wave or DAC waveform). Like the rest of
//! `tamer`, this module is pure and HAL-agnostic: it produces
//! [`ToneOutput`] values, never touches a pin, PWM peripheral, or DAC.
//!
//! # Clock
//!
//! Like [`crate::debounce`] and [`crate::button`], the caller owns the clock:
//! pass monotonic `u64` tick values (milliseconds, microseconds, raw timer
//! counts — your choice, kept consistent between [`start`](ToneSequencer::start)
//! and [`update`](ToneSequencer::update)) to every `update` call. Elapsed time
//! since the current note's baseline uses [`saturating_sub`](u64::saturating_sub),
//! so a non-monotonic timestamp clamps to zero rather than wrapping, delaying
//! an advance instead of producing a spurious one.
//!
//! Note boundaries are **schedule-accumulating**, not poll-time-rebasing: when
//! a note expires, the baseline for the next note advances by that note's own
//! `duration_ticks` rather than snapping to whatever `now` the caller happened
//! to pass. This means delayed or jittery polling never stretches or drifts
//! the melody — the schedule is anchored to [`start`](ToneSequencer::start)'s
//! `now`, plus the sum of every elapsed note's duration, independent of when
//! `update` was actually called. A caller that falls behind (a coarse poll
//! interval, a jittery scheduler) catches up **one note per `update` call**,
//! preserving the "at most one event per call" contract below rather than
//! looping internally to catch up all at once.
//!
//! # Example
//!
//! ```
//! use tamer::tone::{Note, SequenceEvent, SequenceMode, ToneSequencer};
//!
//! // A short two-note chirp, played once.
//! const MELODY: [Note; 2] = [
//!     Note::new(440, 100, 255), // A4 for 100 ticks, full volume
//!     Note::new(880, 50, 255),  // A5 for 50 ticks
//! ];
//!
//! let mut player = ToneSequencer::new(&MELODY, SequenceMode::OneShot);
//! player.start(0);
//!
//! assert_eq!(player.output().frequency_hz, 440);
//! assert_eq!(player.update(50), None); // still within note 0
//! assert_eq!(player.update(100), Some(SequenceEvent::NoteChanged(1)));
//! assert_eq!(player.output().frequency_hz, 880);
//! assert_eq!(player.update(150), Some(SequenceEvent::Finished));
//! assert!(player.is_finished());
//! assert_eq!(player.output().frequency_hz, 0); // silent once finished
//! ```
//!
//! Rests and looping — a tone, a silent rest, then wrap forever:
//!
//! ```
//! use tamer::tone::{Note, SequenceEvent, SequenceMode, ToneSequencer};
//!
//! const RIFF: [Note; 2] = [
//!     Note::new(440, 100, 255), // A4 for 100 ticks
//!     Note::rest(50),           // silence for 50 ticks (a rest is a real step)
//! ];
//!
//! let mut player = ToneSequencer::new(&RIFF, SequenceMode::Loop);
//! player.start(0);
//!
//! assert_eq!(player.output().frequency_hz, 440);
//! assert_eq!(player.update(100), Some(SequenceEvent::NoteChanged(1)));
//! assert_eq!(player.output().frequency_hz, 0); // the rest is silent
//! // End of the table under `Loop` wraps back to note 0 — never `Finished`.
//! assert_eq!(player.update(150), Some(SequenceEvent::NoteChanged(0)));
//! assert_eq!(player.output().frequency_hz, 440);
//! assert!(!player.is_finished());
//! ```

/// A single step in a tone sequence: a frequency held for a duration, at a
/// given amplitude.
///
/// `frequency_hz == 0` denotes a **rest** — silence for `duration_ticks`, still
/// consuming a step in the sequence. Use [`rest`](Self::rest) to construct one
/// explicitly.
///
/// All fields are `pub` so a melody table can be built as a `const` array
/// literal (see the [module example](self)); `Note` carries no invariant that
/// a public constructor needs to enforce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Note {
    /// The tone's frequency in Hz. `0` means a rest (silence).
    pub frequency_hz: u32,
    /// How long this note holds, in caller-defined ticks. `0` is valid — the
    /// step advances on the very next [`update`](ToneSequencer::update) call
    /// with no observable dwell time.
    pub duration_ticks: u64,
    /// Output amplitude/volume, `0..=255`. Interpretation (PWM duty, DAC
    /// scale) is left to the downstream hardware adapter.
    pub amplitude: u8,
}

impl Note {
    /// Creates a new note.
    #[must_use]
    pub const fn new(frequency_hz: u32, duration_ticks: u64, amplitude: u8) -> Self {
        Self {
            frequency_hz,
            duration_ticks,
            amplitude,
        }
    }

    /// Creates a rest (silence) of the given duration.
    ///
    /// Equivalent to `Note::new(0, duration_ticks, 0)`.
    #[must_use]
    pub const fn rest(duration_ticks: u64) -> Self {
        Self::new(0, duration_ticks, 0)
    }
}

/// How a [`ToneSequencer`] behaves once it reaches the end of its note table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SequenceMode {
    /// Play through the notes once, then stop and hold silence.
    OneShot,
    /// Wrap back to note `0` after the last note and keep playing indefinitely.
    Loop,
}

/// The current tone the downstream hardware adapter should be producing.
///
/// A pure value, re-queryable every tick via [`ToneSequencer::output`] — the
/// adapter re-reads it as often as it likes rather than having to catch an
/// edge, mirroring how [`crate::presence::DigitalPresence`] is a re-readable
/// state alongside its edge-based `update`.
///
/// `frequency_hz == 0 && amplitude == 0` is the sequencer's silence value: the
/// player is inactive, finished, or the current note has `frequency_hz == 0`
/// (a [`Note::rest`], or any hand-built note with `frequency_hz == 0` — which
/// [`ToneSequencer::output`] coerces to full silence regardless of amplitude).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ToneOutput {
    /// The frequency to output, in Hz. `0` means silence.
    pub frequency_hz: u32,
    /// The amplitude/volume to output, `0..=255`. `0` means silence.
    pub amplitude: u8,
}

impl ToneOutput {
    /// The silent output (`frequency_hz: 0, amplitude: 0`), returned by
    /// [`ToneSequencer::output`] whenever the sequencer is inactive,
    /// finished, or the current note has `frequency_hz == 0` (a [`Note::rest`],
    /// or any other note with `frequency_hz == 0`). Public so callers can
    /// compare against it directly (e.g. `output() == ToneOutput::SILENT`)
    /// instead of re-deriving the silence check.
    pub const SILENT: Self = Self {
        frequency_hz: 0,
        amplitude: 0,
    };
}

/// An event returned by [`ToneSequencer::update`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SequenceEvent {
    /// The sequencer advanced to a new note; the payload is its index into the
    /// note slice passed to [`ToneSequencer::new`].
    NoteChanged(usize),
    /// A [`SequenceMode::OneShot`] sequence completed its last note. Fires
    /// exactly once; never fires for [`SequenceMode::Loop`].
    Finished,
}

/// Pure tone/duration sequencer — the "melody player" state machine.
///
/// Borrows a `&'notes [Note]` table (zero-alloc, `const`-table friendly — see
/// the [module example](self)) and steps through it on each
/// [`update`](Self::update) call, driven by a caller-owned monotonic clock.
/// [`output`](Self::output) is a separate, side-effect-free query so a
/// downstream adapter can re-read the current frequency/amplitude every tick
/// without needing to catch the [`NoteChanged`](SequenceEvent::NoteChanged)
/// edge.
///
/// A sequencer constructed via [`new`](Self::new) is **inactive** until
/// [`start`](Self::start) is called — [`output`](Self::output) returns silence
/// and [`update`](Self::update) is a no-op until then.
///
/// # Example
///
/// ```
/// use tamer::tone::{Note, SequenceMode, ToneSequencer};
///
/// const BEEP: [Note; 1] = [Note::new(2000, 200, 200)];
/// let mut player = ToneSequencer::new(&BEEP, SequenceMode::Loop);
///
/// assert!(!player.is_active());
/// player.start(0);
/// assert!(player.is_active());
/// assert_eq!(player.output().frequency_hz, 2000);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ToneSequencer<'notes> {
    notes: &'notes [Note],
    mode: SequenceMode,
    active: bool,
    finished: bool,
    index: usize,
    note_since: u64,
}

impl<'notes> ToneSequencer<'notes> {
    /// Creates a new sequencer over the given note table and playback mode.
    ///
    /// The sequencer is **inactive** until [`start`](Self::start) is called;
    /// [`output`](Self::output) returns silence and
    /// [`is_finished`](Self::is_finished) already returns `true` for an empty
    /// `notes` slice (there is nothing to play).
    #[must_use]
    pub const fn new(notes: &'notes [Note], mode: SequenceMode) -> Self {
        Self {
            notes,
            mode,
            active: false,
            finished: notes.is_empty(),
            index: 0,
            note_since: 0,
        }
    }

    /// (Re)starts playback at note `0`, resetting the tick baseline to `now`.
    ///
    /// Calling `start` while already active restarts from the beginning —
    /// there is no "resume" semantics. A [`SequenceMode::OneShot`] sequencer
    /// that had [`Finished`](SequenceEvent::Finished) becomes active and
    /// unfinished again (unless `notes` is empty, which is always finished).
    pub fn start(&mut self, now: u64) {
        self.active = !self.notes.is_empty();
        self.finished = self.notes.is_empty();
        self.index = 0;
        self.note_since = now;
    }

    /// Stops playback: a pause-to-silence, not a terminal state transition.
    ///
    /// [`output`](Self::output) returns silence and [`update`](Self::update)
    /// becomes a no-op until [`start`](Self::start) is called again. Only the
    /// active flag is cleared — the note index and the
    /// [`is_finished`](Self::is_finished) flag are left as-is (neither leaks,
    /// since both `update` and `output` gate on the active flag). The next
    /// [`start`](Self::start) performs the full reset back to note `0`.
    pub fn stop(&mut self) {
        self.active = false;
    }

    /// Advances the sequencer to the current tick.
    ///
    /// Returns `Some(`[`NoteChanged`](SequenceEvent::NoteChanged)`(i))` when
    /// playback advances to note `i`, `Some(`[`Finished`](SequenceEvent::Finished)`)`
    /// exactly once when a [`SequenceMode::OneShot`] sequence completes its
    /// last note, or `None` otherwise (including whenever the sequencer is
    /// inactive or already finished).
    ///
    /// A note boundary is `elapsed >= duration_ticks`; a zero-duration note
    /// therefore advances on the very next call with no infinite inner loop —
    /// at most one note-changed event is emitted per `update`, so a train of
    /// zero-duration notes is walked one call at a time, mirroring
    /// [`crate::button::ButtonDecoder`]'s "at most one event per call"
    /// contract.
    ///
    /// On advancing, the note baseline accumulates the *expiring* note's own
    /// `duration_ticks` rather than rebasing to `now` (see the module-level
    /// `# Clock` section) — so a late or coarse `now` never stretches the
    /// note that just elapsed. A sequence that fell behind catches up one
    /// note per `update` call rather than all at once.
    #[must_use]
    pub fn update(&mut self, now: u64) -> Option<SequenceEvent> {
        if !self.active || self.finished {
            return None;
        }

        let Some(current) = self.notes.get(self.index) else {
            // Defensive: an empty slice is caught by `finished` above, and
            // `index` never advances past `notes.len()` elsewhere, but this
            // guard keeps `update` panic-free under any future change.
            self.active = false;
            return None;
        };

        if now.saturating_sub(self.note_since) < current.duration_ticks {
            return None;
        }

        // Boundary reached: advance to the next note. The baseline advances by
        // the *expiring* note's own duration rather than rebasing to `now`, so
        // a late or coarse poll doesn't stretch the note that just elapsed —
        // see the schedule-accumulating semantics documented above.
        if self.index + 1 < self.notes.len() {
            self.note_since = self.note_since.saturating_add(current.duration_ticks);
            self.index += 1;
            return Some(SequenceEvent::NoteChanged(self.index));
        }

        // Reached the end of the table.
        match self.mode {
            SequenceMode::Loop => {
                self.note_since = self.note_since.saturating_add(current.duration_ticks);
                self.index = 0;
                Some(SequenceEvent::NoteChanged(0))
            }
            SequenceMode::OneShot => {
                self.active = false;
                self.finished = true;
                Some(SequenceEvent::Finished)
            }
        }
    }

    /// Returns the tone the downstream adapter should currently be producing.
    ///
    /// Returns [`ToneOutput::SILENT`] when inactive, finished, or the current
    /// note's `frequency_hz` is `0` (a [`Note::rest`], or any other note with
    /// `frequency_hz == 0` regardless of its `amplitude` — 0 Hz can't
    /// meaningfully sound, so it's always coerced to full silence); otherwise
    /// the current note's frequency and amplitude verbatim.
    #[must_use]
    pub fn output(&self) -> ToneOutput {
        if !self.active || self.finished {
            return ToneOutput::SILENT;
        }
        match self.notes.get(self.index) {
            Some(note) if note.frequency_hz == 0 => ToneOutput::SILENT,
            Some(note) => ToneOutput {
                frequency_hz: note.frequency_hz,
                amplitude: note.amplitude,
            },
            None => ToneOutput::SILENT,
        }
    }

    /// Returns `true` if a [`SequenceMode::OneShot`] sequence has completed.
    ///
    /// An empty note slice is always `true` regardless of mode (nothing to
    /// play). For a non-empty sequence, [`SequenceMode::Loop`] never becomes
    /// `finished`.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Returns `true` if the sequencer is currently playing (started and not
    /// yet finished or stopped).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Note ---

    #[test]
    fn rest_is_zero_frequency_and_amplitude() {
        let r = Note::rest(100);
        assert_eq!(r.frequency_hz, 0);
        assert_eq!(r.amplitude, 0);
        assert_eq!(r.duration_ticks, 100);
    }

    #[test]
    fn new_stores_fields_verbatim() {
        let n = Note::new(440, 100, 200);
        assert_eq!(n.frequency_hz, 440);
        assert_eq!(n.duration_ticks, 100);
        assert_eq!(n.amplitude, 200);
    }

    // --- Basic playback + exact boundary ---

    #[test]
    fn basic_playback_and_note_changed_at_exact_boundary() {
        const NOTES: [Note; 2] = [Note::new(440, 100, 255), Note::new(880, 50, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        p.start(0);

        assert_eq!(p.output().frequency_hz, 440);
        // One tick before the boundary: no change (off-by-one check).
        assert_eq!(p.update(99), None);
        assert_eq!(p.output().frequency_hz, 440);
        // Exactly at the boundary (elapsed == duration): advances.
        assert_eq!(p.update(100), Some(SequenceEvent::NoteChanged(1)));
        assert_eq!(p.output().frequency_hz, 880);
    }

    #[test]
    fn one_tick_past_boundary_still_advances() {
        const NOTES: [Note; 2] = [Note::new(440, 100, 255), Note::new(880, 50, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        p.start(0);
        assert_eq!(p.update(101), Some(SequenceEvent::NoteChanged(1)));
    }

    // --- Schedule-accumulating boundaries (no drift under jitter/coarse polling) ---

    #[test]
    fn coarse_polling_preserves_schedule_without_drift() {
        // Three 100-tick notes: schedule boundaries at t=100, t=200, t=300.
        const NOTES: [Note; 3] = [
            Note::new(440, 100, 255),
            Note::new(880, 100, 255),
            Note::new(220, 100, 255),
        ];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        p.start(0);

        // A single coarse poll at t=250 is 150 ticks past the first boundary
        // (t=100). Only ONE note-changed event fires per `update` call, so
        // this advances exactly to note 1 — it does not skip ahead to note 2
        // even though 250 >= 200 as well.
        assert_eq!(p.update(250), Some(SequenceEvent::NoteChanged(1)));
        assert_eq!(p.output().frequency_hz, 880);

        // Schedule-accumulating semantics: note 1's baseline is anchored at
        // the *schedule* (t=100, note 0's own duration), not at the t=250
        // poll time. Its boundary is therefore t=100+100=200, which the next
        // poll at t=250 has already passed — so a following `update(250)`
        // immediately advances again instead of waiting another full 100
        // ticks from t=250 (which the old rebase-to-`now` behavior would
        // have done: `note_since` would have been set to 250, pushing the
        // next boundary out to t=350).
        assert_eq!(p.update(250), Some(SequenceEvent::NoteChanged(2)));
        assert_eq!(p.output().frequency_hz, 220);

        // Note 2's schedule boundary is t=200+100=300, which has NOT yet
        // been reached by t=250 (already accounted for above) — confirm no
        // further, spurious advance at the same poll time.
        assert_eq!(p.update(250), None);
        assert_eq!(p.output().frequency_hz, 220);

        // Finishes exactly at the true schedule boundary, t=300.
        assert_eq!(p.update(300), Some(SequenceEvent::Finished));
    }

    #[test]
    fn jitter_does_not_accumulate() {
        // A 100-tick note followed by another 100-tick note: schedule
        // boundaries at t=100 and t=200.
        const NOTES: [Note; 3] = [
            Note::new(440, 100, 255),
            Note::new(880, 100, 255),
            Note::new(220, 100, 255),
        ];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        p.start(0);

        // Poll slightly late (5-tick jitter past the t=100 boundary).
        assert_eq!(p.update(105), Some(SequenceEvent::NoteChanged(1)));
        assert_eq!(p.output().frequency_hz, 880);

        // The 5 ticks of lateness must NOT be baked into the schedule: note
        // 1's boundary stays at the schedule-relative t=100+100=200, not
        // t=105+100=205. One tick early (t=199) must not advance yet.
        assert_eq!(p.update(199), None);
        assert_eq!(p.output().frequency_hz, 880);
        // Exactly at the true schedule boundary, t=200, it advances.
        assert_eq!(p.update(200), Some(SequenceEvent::NoteChanged(2)));
        assert_eq!(p.output().frequency_hz, 220);
    }

    #[test]
    fn loop_wrap_schedule_survives_coarse_polling() {
        // The `Loop` wrap branch has its own accumulate statement, so it needs
        // its own coarse-poll coverage. Two 10-tick notes: schedule boundaries
        // at t=10, 20, 30, 40, 50, ...
        const NOTES: [Note; 2] = [Note::new(440, 10, 255), Note::new(880, 10, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::Loop);
        p.start(0);

        // Coarse poll past the first boundary (t=10) but before the second.
        assert_eq!(p.update(15), Some(SequenceEvent::NoteChanged(1)));
        // The wrap boundary is schedule-anchored at t=20, NOT poll-anchored at
        // t=15+10=25 — so this same-tick poll must not advance yet.
        assert_eq!(p.update(15), None);
        // At the true schedule boundary t=20 the sequence wraps to note 0.
        assert_eq!(p.update(20), Some(SequenceEvent::NoteChanged(0)));

        // A genuinely coarse poll at t=45 walks two schedule boundaries
        // (t=30 then t=40), one per call, and stops before t=50.
        assert_eq!(p.update(45), Some(SequenceEvent::NoteChanged(1)));
        assert_eq!(p.update(45), Some(SequenceEvent::NoteChanged(0)));
        assert_eq!(p.update(45), None);
        assert!(!p.is_finished());
    }

    // --- Rests ---

    #[test]
    fn rest_outputs_silence_while_active_then_advances() {
        const NOTES: [Note; 2] = [Note::rest(50), Note::new(440, 50, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        p.start(0);

        assert_eq!(
            p.output(),
            ToneOutput {
                frequency_hz: 0,
                amplitude: 0
            }
        );
        assert_eq!(p.update(25), None);
        assert_eq!(p.output().frequency_hz, 0);
        assert_eq!(p.update(50), Some(SequenceEvent::NoteChanged(1)));
        assert_eq!(p.output().frequency_hz, 440);
    }

    #[test]
    fn zero_hz_note_with_nonzero_amplitude_outputs_silent() {
        // Not constructed via `Note::rest` — a hand-built note with a nonzero
        // amplitude but `frequency_hz == 0` must still coerce to full silence.
        const NOTES: [Note; 1] = [Note::new(0, 100, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        p.start(0);

        assert_eq!(p.output(), ToneOutput::SILENT);
    }

    // --- Zero-duration notes ---

    #[test]
    fn zero_duration_note_advances_on_next_update_without_looping_forever() {
        const NOTES: [Note; 3] = [
            Note::new(100, 0, 255),
            Note::new(200, 0, 255),
            Note::new(300, 10, 255),
        ];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        p.start(0);

        assert_eq!(p.output().frequency_hz, 100);
        // A single `update` call advances at most one step, even though the
        // next note also has zero duration.
        assert_eq!(p.update(0), Some(SequenceEvent::NoteChanged(1)));
        assert_eq!(p.output().frequency_hz, 200);
        assert_eq!(p.update(0), Some(SequenceEvent::NoteChanged(2)));
        assert_eq!(p.output().frequency_hz, 300);
        // The last note has a real duration, so it holds now.
        assert_eq!(p.update(0), None);
        assert_eq!(p.update(10), Some(SequenceEvent::Finished));
    }

    #[test]
    fn accumulate_survives_zero_duration_notes_under_coarse_polling() {
        // The scenario the old rebase-to-`now` logic got wrong: two
        // zero-duration notes keep the schedule anchor at t=0, so the real
        // note's true boundary stays at t=10 no matter how far past it `now`
        // is polled. Rebasing to `now` on a zero-duration advance would have
        // pushed that boundary out to `now + 10`, wrongly delaying expiry.
        const NOTES: [Note; 3] = [
            Note::new(100, 0, 255),
            Note::new(200, 0, 255),
            Note::new(300, 10, 255),
        ];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        p.start(0);

        // The same coarse `now` (1_000) on three successive calls: each
        // advances one note, the anchor stays at 0 (0 + 0 + 0), so note 2
        // still expires against the true t=10 boundary — 1_000 is well past
        // it — rather than being pushed out to 1_000 + 10.
        assert_eq!(p.update(1_000), Some(SequenceEvent::NoteChanged(1)));
        assert_eq!(p.update(1_000), Some(SequenceEvent::NoteChanged(2)));
        assert_eq!(p.update(1_000), Some(SequenceEvent::Finished));
    }

    // --- OneShot completion ---

    #[test]
    fn oneshot_finishes_exactly_once_then_stays_silent() {
        const NOTES: [Note; 1] = [Note::new(440, 100, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        p.start(0);

        assert_eq!(p.update(100), Some(SequenceEvent::Finished));
        assert!(p.is_finished());
        assert_eq!(
            p.output(),
            ToneOutput {
                frequency_hz: 0,
                amplitude: 0
            }
        );

        // Subsequent updates are inert.
        assert_eq!(p.update(200), None);
        assert_eq!(p.update(1_000_000), None);
        assert!(p.is_finished());
        assert_eq!(p.output().frequency_hz, 0);
    }

    // --- Loop wraps, never finishes ---

    #[test]
    fn loop_wraps_to_note_zero_and_never_finishes() {
        const NOTES: [Note; 2] = [Note::new(440, 10, 255), Note::new(880, 10, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::Loop);
        p.start(0);

        // First full cycle.
        assert_eq!(p.update(10), Some(SequenceEvent::NoteChanged(1)));
        assert_eq!(p.update(20), Some(SequenceEvent::NoteChanged(0)));
        assert!(!p.is_finished());
        // Second full cycle.
        assert_eq!(p.update(30), Some(SequenceEvent::NoteChanged(1)));
        assert_eq!(p.update(40), Some(SequenceEvent::NoteChanged(0)));
        assert!(!p.is_finished());
        assert!(p.is_active());
    }

    #[test]
    fn loop_mode_all_zero_duration_advances_one_step_per_update() {
        const NOTES: [Note; 2] = [Note::new(100, 0, 255), Note::new(200, 0, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::Loop);
        p.start(0);
        assert_eq!(p.update(0), Some(SequenceEvent::NoteChanged(1)));
        assert_eq!(p.update(0), Some(SequenceEvent::NoteChanged(0)));
        assert!(!p.is_finished());
    }

    // --- Empty slice ---

    #[test]
    fn empty_slice_is_finished_immediately_and_never_panics() {
        const NOTES: [Note; 0] = [];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);

        assert!(p.is_finished());
        assert_eq!(
            p.output(),
            ToneOutput {
                frequency_hz: 0,
                amplitude: 0
            }
        );

        // start()/update() on an empty table must not panic or index OOB.
        p.start(0);
        assert!(p.is_finished());
        assert!(!p.is_active());
        assert_eq!(p.update(0), None);
        assert_eq!(p.output().frequency_hz, 0);
    }

    #[test]
    fn empty_slice_loop_mode_is_also_finished() {
        const NOTES: [Note; 0] = [];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::Loop);
        assert!(p.is_finished());
        p.start(0);
        assert!(p.is_finished());
        assert_eq!(p.update(0), None);
    }

    // --- Non-monotonic `now` ---

    #[test]
    fn non_monotonic_now_saturates_without_panic_or_spurious_advance() {
        const NOTES: [Note; 2] = [Note::new(440, 100, 255), Note::new(880, 50, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        p.start(100);

        // `now` earlier than the note's start: saturating_sub clamps to 0.
        assert_eq!(p.update(50), None);
        assert_eq!(p.output().frequency_hz, 440);
        // Still doesn't advance until the real boundary from note_since=100.
        assert_eq!(p.update(150), None);
        assert_eq!(p.update(200), Some(SequenceEvent::NoteChanged(1)));
    }

    // --- Near-u64::MAX ---

    #[test]
    fn near_u64_max_start_transitions_correctly() {
        const NOTES: [Note; 2] = [Note::new(440, 50, 255), Note::new(880, 50, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        let start = u64::MAX - 50;
        p.start(start);

        assert_eq!(p.update(u64::MAX - 1), None);
        assert_eq!(p.update(u64::MAX), Some(SequenceEvent::NoteChanged(1)));
        assert_eq!(p.output().frequency_hz, 880);
    }

    #[test]
    fn near_u64_max_oneshot_finishes() {
        const NOTES: [Note; 1] = [Note::new(440, 10, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        p.start(u64::MAX - 10);
        assert_eq!(p.update(u64::MAX), Some(SequenceEvent::Finished));
        assert!(p.is_finished());
    }

    // --- start() restarts from note 0 ---

    #[test]
    fn start_while_active_restarts_from_note_zero_and_resets_baseline() {
        const NOTES: [Note; 2] = [Note::new(440, 100, 255), Note::new(880, 100, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        p.start(0);
        assert_eq!(p.update(100), Some(SequenceEvent::NoteChanged(1)));
        assert_eq!(p.output().frequency_hz, 880);

        // Restart mid-sequence: back to note 0, baseline reset to the new `now`.
        p.start(1_000);
        assert_eq!(p.output().frequency_hz, 440);
        assert!(p.is_active());
        assert!(!p.is_finished());
        // Baseline is 1_000, not 0 — an old-baseline elapsed check would fire early.
        assert_eq!(p.update(1_050), None);
        assert_eq!(p.update(1_100), Some(SequenceEvent::NoteChanged(1)));
    }

    #[test]
    fn start_after_finished_resets_finished_flag() {
        const NOTES: [Note; 1] = [Note::new(440, 10, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);
        p.start(0);
        assert_eq!(p.update(10), Some(SequenceEvent::Finished));
        assert!(p.is_finished());

        p.start(100);
        assert!(!p.is_finished());
        assert!(p.is_active());
        assert_eq!(p.output().frequency_hz, 440);
    }

    // --- stop() ---

    #[test]
    fn stop_silences_output_and_suspends_update() {
        const NOTES: [Note; 1] = [Note::new(440, 100, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::Loop);
        p.start(0);
        assert!(p.is_active());

        p.stop();
        assert!(!p.is_active());
        assert_eq!(p.output().frequency_hz, 0);
        // update() is a no-op while stopped.
        assert_eq!(p.update(1_000), None);
        assert_eq!(p.output().frequency_hz, 0);
    }

    // --- inactive-before-start ---

    #[test]
    fn inactive_before_start_is_silent_and_update_is_noop() {
        const NOTES: [Note; 1] = [Note::new(440, 100, 255)];
        let mut p = ToneSequencer::new(&NOTES, SequenceMode::OneShot);

        assert!(!p.is_active());
        assert!(!p.is_finished());
        assert_eq!(p.output().frequency_hz, 0);
        assert_eq!(p.update(0), None);
        assert_eq!(p.update(1_000), None);
    }
}
