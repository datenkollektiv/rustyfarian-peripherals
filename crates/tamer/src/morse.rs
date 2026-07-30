//! Morse-code keyer — [`code_for`](crate::morse::code_for), [`MorseKeyer`], [`MorseMode`],
//! [`MorseEvent`], and [`KeyState`].
//!
//! A [`MorseKeyer`] steps through a caller-owned, borrowed ASCII `&[u8]`
//! message, encoding it as International Morse code (ITU-R M.1677-1) and
//! reporting the on/off key state a downstream buzzer/LED adapter should be
//! driving, via a separate [`output`](MorseKeyer::output) query. Like the
//! rest of `tamer`, this module is pure and HAL-agnostic: it produces
//! [`KeyState`] values, never touches a pin or GPIO peripheral.
//!
//! # Clock
//!
//! Like [`crate::tone`], the caller owns the clock: pass monotonic `u64` tick
//! values to every [`start`](MorseKeyer::start)/[`update`](MorseKeyer::update)
//! call. Elapsed time since the current segment's baseline uses
//! [`saturating_sub`](u64::saturating_sub), so a non-monotonic timestamp
//! clamps to zero rather than wrapping, delaying an advance instead of
//! producing a spurious one.
//!
//! Segment boundaries are **schedule-accumulating**, not poll-time-rebasing:
//! when a segment (a dot, a dash, or a gap) expires, the baseline for the
//! next segment advances by that segment's own duration
//! (`since = since.saturating_add(duration)`) rather than snapping to
//! whatever `now` the caller happened to pass. A caller that falls behind
//! catches up **one segment per `update` call**, preserving the "at most one
//! event per call" contract below rather than looping internally to catch up
//! all at once — poll substantially faster than `unit_ticks` (a keying dot)
//! to avoid audible lag.
//!
//! `unit_ticks == 0` is supported only as a degenerate, panic-free case:
//! every segment then has zero duration, so each `update` call advances
//! exactly one segment immediately, with no dwell time at all. It produces
//! no audible keying and is not meaningful for a real caller — it exists so
//! the zero-duration path is exercised and never panics, mirroring
//! [`crate::tone::ToneSequencer`]'s zero-duration-note contract.
//!
//! # Example
//!
//! `CQ` in Morse is `-.-.` `--.-`; at `unit_ticks = 10` a dot is 10 ticks and
//! a dash is 30. `update` advances at most one segment per call, so a real
//! caller polls in a loop until it sees the event it's waiting for — the
//! `None`s below are C's three intra-character gaps and its own three other
//! symbols, followed by the inter-character gap before Q begins:
//!
//! ```
//! use tamer::morse::{KeyState, MorseEvent, MorseKeyer, MorseMode};
//!
//! let mut keyer = MorseKeyer::new(b"CQ", 10, MorseMode::OneShot);
//! keyer.start(0);
//!
//! // C's first dash is already loaded by `start` — no event for it.
//! assert_eq!(keyer.output(), KeyState::On);
//!
//! for _ in 0..7 {
//!     assert_eq!(keyer.update(140), None);
//! }
//! assert_eq!(keyer.update(140), Some(MorseEvent::CharacterStarted(1)));
//! assert_eq!(keyer.output(), KeyState::On);
//!
//! for _ in 0..6 {
//!     assert_eq!(keyer.update(270), None);
//! }
//! assert_eq!(keyer.update(270), Some(MorseEvent::Finished));
//! assert!(keyer.is_finished());
//! assert_eq!(keyer.output(), KeyState::Off);
//! ```
//!
//! [`MorseMode::Loop`] never finishes; reaching the end of the message wraps
//! back to the first keyed character after exactly one 7-unit word gap, the
//! "loop seam", regardless of any spaces at the message's edges:
//!
//! ```
//! use tamer::morse::{KeyState, MorseEvent, MorseKeyer, MorseMode};
//!
//! let mut keyer = MorseKeyer::new(b"E", 10, MorseMode::Loop);
//! keyer.start(0);
//! assert_eq!(keyer.output(), KeyState::On); // E's dot, loaded by `start`
//!
//! assert_eq!(keyer.update(10), None); // the dot ends; enters the seam gap
//! assert_eq!(keyer.output(), KeyState::Off);
//! assert_eq!(keyer.update(80), Some(MorseEvent::CharacterStarted(0)));
//! assert_eq!(keyer.output(), KeyState::On);
//! assert!(!keyer.is_finished());
//! ```

/// A dot's duration, in [`unit_ticks`](MorseKeyer::new).
pub const DOT_UNITS: u64 = 1;
/// A dash's duration, in [`unit_ticks`](MorseKeyer::new).
pub const DASH_UNITS: u64 = 3;
/// The gap between two symbols (dots/dashes) within one character, in
/// [`unit_ticks`](MorseKeyer::new).
pub const INTRA_CHAR_GAP_UNITS: u64 = 1;
/// The gap between two characters of the same word, in
/// [`unit_ticks`](MorseKeyer::new).
pub const INTER_CHAR_GAP_UNITS: u64 = 3;
/// The gap for a word (space), in [`unit_ticks`](MorseKeyer::new). Runs of
/// spaces and skipped bytes collapse to a single word gap — see the
/// [module-level `# Clock`](self) section and [`MorseKeyer`]'s docs.
pub const WORD_GAP_UNITS: u64 = 7;

/// Returns the ITU-R M.1677-1 Morse pattern for an ASCII letter or digit.
///
/// Uppercase-folds the input, so `code_for(b'q')` and `code_for(b'Q')` both
/// return `Q`'s pattern. Returns `None` for any byte outside
/// `A-Z`/`a-z`/`0-9` — including space, which [`MorseKeyer`] handles
/// separately as a word gap rather than a character.
///
/// A `const fn match` over `u8` compiles to a compact jump table and avoids
/// ASCII-gap index arithmetic.
///
/// # Examples
///
/// ```
/// use tamer::morse::code_for;
///
/// assert_eq!(code_for(b'C'), Some("-.-."));
/// assert_eq!(code_for(b'Q'), Some("--.-"));
/// assert_eq!(code_for(b'q'), Some("--.-")); // lowercase folds to `Q`
/// assert_eq!(code_for(b' '), None); // a space is a word gap, not a code
/// assert_eq!(code_for(b'!'), None); // unsupported: punctuation is v1 out of scope
/// ```
#[must_use]
pub const fn code_for(byte: u8) -> Option<&'static str> {
    match byte.to_ascii_uppercase() {
        b'A' => Some(".-"),
        b'B' => Some("-..."),
        b'C' => Some("-.-."),
        b'D' => Some("-.."),
        b'E' => Some("."),
        b'F' => Some("..-."),
        b'G' => Some("--."),
        b'H' => Some("...."),
        b'I' => Some(".."),
        b'J' => Some(".---"),
        b'K' => Some("-.-"),
        b'L' => Some(".-.."),
        b'M' => Some("--"),
        b'N' => Some("-."),
        b'O' => Some("---"),
        b'P' => Some(".--."),
        b'Q' => Some("--.-"),
        b'R' => Some(".-."),
        b'S' => Some("..."),
        b'T' => Some("-"),
        b'U' => Some("..-"),
        b'V' => Some("...-"),
        b'W' => Some(".--"),
        b'X' => Some("-..-"),
        b'Y' => Some("-.--"),
        b'Z' => Some("--.."),
        b'0' => Some("-----"),
        b'1' => Some(".----"),
        b'2' => Some("..---"),
        b'3' => Some("...--"),
        b'4' => Some("....-"),
        b'5' => Some("....."),
        b'6' => Some("-...."),
        b'7' => Some("--..."),
        b'8' => Some("---.."),
        b'9' => Some("----."),
        _ => None,
    }
}

/// Scans `message[from..]` for the byte offset of the next encodable
/// character, folding over unsupported bytes and spaces along the way.
///
/// Returns the found offset (or `None` if the message has no more encodable
/// characters from `from` onward) plus whether a space byte was seen during
/// the scan. A run of spaces and skip-bytes is walked as a single pass, so
/// the caller never needs to re-scan to find out whether a gap should be a
/// word gap — this is the one place that answers both questions at once.
const fn scan_next(message: &[u8], from: usize) -> (Option<usize>, bool) {
    let mut i = from;
    let mut saw_space = false;
    while i < message.len() {
        let byte = message[i];
        if let Some(_code) = code_for(byte) {
            return (Some(i), saw_space);
        }
        if byte == b' ' {
            saw_space = true;
        }
        i += 1;
    }
    (None, saw_space)
}

/// The shape of a message's keyable content, classified once by [`classify`].
#[derive(Debug, Clone, Copy)]
enum Content {
    /// No encodable character and no space: nothing to key, ever.
    Empty,
    /// No encodable character, but at least one space: timed silence.
    SpacesOnly,
    /// At least one encodable character, at the given byte offset. `leading_space`
    /// is whether a space preceded it (only meaningful to [`MorseMode::OneShot`]).
    Keyed { offset: usize, leading_space: bool },
}

/// Classifies a message's keyable content in a single scan from the start.
const fn classify(message: &[u8]) -> Content {
    let (first, saw_space) = scan_next(message, 0);
    match first {
        Some(offset) => Content::Keyed {
            offset,
            leading_space: saw_space,
        },
        None if saw_space => Content::SpacesOnly,
        None => Content::Empty,
    }
}

/// The duration of a dot or dash, in ticks, at the given `unit_ticks`.
///
/// Uses [`saturating_mul`](u64::saturating_mul) so a pathologically large
/// `unit_ticks` never overflow-panics; the result simply saturates at
/// `u64::MAX`, which just stalls further advancement rather than wrapping.
const fn symbol_duration_ticks(symbol: u8, unit_ticks: u64) -> u64 {
    let units = if symbol == b'.' {
        DOT_UNITS
    } else {
        DASH_UNITS
    };
    units.saturating_mul(unit_ticks)
}

/// The duration of a gap, in ticks, at the given `unit_ticks`. See
/// [`symbol_duration_ticks`] for the saturating-multiplication rationale.
const fn gap_duration_ticks(kind: GapKind, unit_ticks: u64) -> u64 {
    let units = match kind {
        GapKind::Intra => INTRA_CHAR_GAP_UNITS,
        GapKind::InterChar => INTER_CHAR_GAP_UNITS,
        GapKind::Word => WORD_GAP_UNITS,
    };
    units.saturating_mul(unit_ticks)
}

/// Which kind of gap a [`Segment::Gap`] is timing.
#[derive(Debug, Clone, Copy)]
enum GapKind {
    /// Between two symbols (dots/dashes) of the same character.
    Intra,
    /// Between two characters, no space between them.
    InterChar,
    /// A word gap: a space, a run of spaces/skip-bytes, or a loop seam.
    Word,
}

/// What a [`Segment::Gap`] transitions into once it elapses.
#[derive(Debug, Clone, Copy)]
enum GapTarget {
    /// Resume the *same* character at `symbol_idx`, without firing
    /// [`CharacterStarted`](MorseEvent::CharacterStarted). Used for
    /// intra-character symbol gaps, and for a leading word gap's transition
    /// into the very first keyed character — which was already "loaded" by
    /// [`start`](MorseKeyer::start), so its arrival is silent even though a
    /// gap delayed it.
    Continue {
        char_offset: usize,
        symbol_idx: usize,
    },
    /// Begin a *new* character at this byte offset, firing
    /// [`CharacterStarted`](MorseEvent::CharacterStarted). Used both for the
    /// next distinct character in the same pass and for a [`MorseMode::Loop`]
    /// wrap back to the first keyed character.
    CharacterStart(usize),
    /// End the sequence ([`MorseMode::OneShot`] only).
    Finished,
}

/// Where a [`MorseKeyer`] currently is in its segment stream.
///
/// A "segment" is one dot, one dash, or one gap — the smallest unit
/// [`MorseKeyer::update`] advances by. The stream is walked lazily: this
/// enum plus [`MorseKeyer::message`] is enough to derive the next segment on
/// demand, so nothing is ever materialized.
#[derive(Debug, Clone, Copy)]
enum Segment {
    /// Playing symbol `symbol_idx` of the character at byte offset `char_offset`.
    Symbol {
        char_offset: usize,
        symbol_idx: usize,
    },
    /// Holding silent for a gap, which will transition to `target` once it elapses.
    Gap { kind: GapKind, target: GapTarget },
    /// Permanently silent: only reachable before the first [`start`](MorseKeyer::start)
    /// call, and for a [`MorseMode::Loop`] keyer over an all-space message
    /// (no character ever exists to key, so nothing ever advances).
    Idle,
}

/// The on/off key state a downstream buzzer/LED adapter should be driving.
///
/// A pure value, re-queryable every tick via [`MorseKeyer::output`] — the
/// adapter re-reads it as often as it likes rather than having to catch an
/// edge, mirroring [`crate::tone::ToneOutput`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyState {
    /// The key/buzzer/LED should be off (silent).
    Off,
    /// The key/buzzer/LED should be on (a dot or dash is sounding).
    On,
}

/// How a [`MorseKeyer`] behaves once it reaches the end of its message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MorseMode {
    /// Key the message once, then stop and hold silence.
    OneShot,
    /// Wrap back to the first keyed character after the message ends, and
    /// keep keying indefinitely. The wrap always inserts exactly one 7-unit
    /// word gap — the "loop seam" — collapsing any spaces at either edge of
    /// the message; see [`MorseKeyer`]'s docs for the full contract.
    Loop,
}

/// An event returned by [`MorseKeyer::update`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MorseEvent {
    /// The keyer began keying a new character; the payload is that
    /// character's byte offset in the `message` slice passed to
    /// [`MorseKeyer::new`] — not a count of characters played, since
    /// unsupported bytes and spaces can separate it from the previous one.
    ///
    /// The very first keyed character never fires this event via `update`,
    /// because it is already loaded by [`start`](MorseKeyer::start) — the
    /// caller logs it explicitly right after calling `start`.
    CharacterStarted(usize),
    /// A [`MorseMode::OneShot`] keyer reached the end of its message. Fires
    /// exactly once; never fires for [`MorseMode::Loop`].
    Finished,
}

/// Pure Morse-code keyer — the "active buzzer" state machine.
///
/// Borrows a `&'msg [u8]` ASCII message (zero-alloc — see the
/// [module example](self)) and steps through its Morse encoding on each
/// [`update`](Self::update) call, driven by a caller-owned monotonic clock.
/// [`output`](Self::output) is a separate, side-effect-free query so a
/// downstream adapter can re-read the current key state every tick without
/// needing to catch an event.
///
/// A keyer constructed via [`new`](Self::new) is **inactive** until
/// [`start`](Self::start) is called — [`output`](Self::output) returns
/// [`KeyState::Off`] and [`update`](Self::update) is a no-op until then.
///
/// # Unsupported bytes and spaces
///
/// Any byte outside `A-Z`/`a-z`/`0-9`/space is silently skipped: no segment,
/// no gap, no event, no tick cost — `b"C!Q"` keys identically to `b"CQ"`. A
/// space is meaningful timed content: it produces a 7-unit word gap, and a
/// run of spaces and skip-bytes collapses to a single word gap rather than
/// stacking. An empty message, or one with no encodable character and no
/// space, has no keyable content: [`is_finished`](Self::is_finished) is
/// `true` immediately. An all-space message is still timed content: a
/// [`MorseMode::OneShot`] keyer holds [`KeyState::Off`] for one word gap then
/// fires [`Finished`](MorseEvent::Finished) once; a [`MorseMode::Loop`] keyer
/// stays [`KeyState::Off`] forever, never finishes, and emits no events.
///
/// # Example
///
/// ```
/// use tamer::morse::{KeyState, MorseKeyer, MorseMode};
///
/// let mut keyer = MorseKeyer::new(b"SOS", 60, MorseMode::Loop);
///
/// assert!(!keyer.is_active());
/// keyer.start(0);
/// assert!(keyer.is_active());
/// assert_eq!(keyer.output(), KeyState::On); // `S` is dot-dot-dot
/// ```
#[derive(Debug, Clone, Copy)]
pub struct MorseKeyer<'msg> {
    message: &'msg [u8],
    unit_ticks: u64,
    mode: MorseMode,
    active: bool,
    finished: bool,
    segment: Segment,
    segment_since: u64,
}

impl<'msg> MorseKeyer<'msg> {
    /// Creates a new keyer over the given message, dot length, and mode.
    ///
    /// `unit_ticks` is the dot length in caller-defined ticks (a dash is
    /// three units; gaps are one/three/seven units — see the module-level
    /// `*_UNITS` constants). `unit_ticks == 0` is accepted only as a
    /// degenerate, panic-free case: see the module-level `# Clock` section.
    ///
    /// The keyer is **inactive** until [`start`](Self::start) is called;
    /// [`output`](Self::output) returns [`KeyState::Off`] and
    /// [`is_finished`](Self::is_finished) already returns `true` for a
    /// message with no keyable content (empty, or no encodable character and
    /// no space — there is nothing to key).
    #[must_use]
    pub const fn new(message: &'msg [u8], unit_ticks: u64, mode: MorseMode) -> Self {
        let finished = match classify(message) {
            Content::Empty => true,
            Content::SpacesOnly | Content::Keyed { .. } => false,
        };
        Self {
            message,
            unit_ticks,
            mode,
            active: false,
            finished,
            segment: Segment::Idle,
            segment_since: 0,
        }
    }

    /// (Re)starts keying at the first keyed character, resetting the tick
    /// baseline to `now`.
    ///
    /// Calling `start` while already active restarts from the beginning —
    /// there is no "resume" semantics. A [`MorseMode::OneShot`] keyer that
    /// had [`Finished`](MorseEvent::Finished) becomes active and unfinished
    /// again (unless the message has no keyable content, which is always
    /// finished).
    ///
    /// A [`MorseMode::Loop`] keyer always begins keying immediately, even if
    /// the message starts with a space — see [`MorseKeyer`]'s docs. A
    /// [`MorseMode::OneShot`] keyer honors a leading space literally, holding
    /// [`KeyState::Off`] for one word gap before its first symbol; in either
    /// case, the first keyed character is "loaded" by this call and does not
    /// fire [`CharacterStarted`](MorseEvent::CharacterStarted) via the next
    /// [`update`](Self::update).
    ///
    /// # Examples
    ///
    /// Leading space, contrasted by mode (`unit_ticks = 10`, message `b" E"`
    /// — a space, then `E`, whose code is a single dot):
    ///
    /// ```
    /// use tamer::morse::{KeyState, MorseKeyer, MorseMode};
    ///
    /// // `OneShot` honors the leading space literally: it holds `Off` for
    /// // one 7-unit word gap (70 ticks) before `E`'s dot begins.
    /// let mut one_shot = MorseKeyer::new(b" E", 10, MorseMode::OneShot);
    /// one_shot.start(0);
    /// assert_eq!(one_shot.output(), KeyState::Off);
    /// assert_eq!(one_shot.update(69), None);
    /// assert_eq!(one_shot.update(70), None); // gap elapses -> `E`'s dot, silently
    /// assert_eq!(one_shot.output(), KeyState::On);
    ///
    /// // `Loop` collapses the same leading space: it begins keying immediately.
    /// let mut looped = MorseKeyer::new(b" E", 10, MorseMode::Loop);
    /// looped.start(0);
    /// assert_eq!(looped.output(), KeyState::On);
    /// ```
    pub fn start(&mut self, now: u64) {
        self.segment_since = now;
        match classify(self.message) {
            Content::Empty => {
                self.active = false;
                self.finished = true;
                self.segment = Segment::Idle;
            }
            Content::SpacesOnly => {
                self.active = true;
                self.finished = false;
                self.segment = match self.mode {
                    MorseMode::OneShot => Segment::Gap {
                        kind: GapKind::Word,
                        target: GapTarget::Finished,
                    },
                    MorseMode::Loop => Segment::Idle,
                };
            }
            Content::Keyed {
                offset,
                leading_space,
            } => {
                self.active = true;
                self.finished = false;
                self.segment = match (self.mode, leading_space) {
                    (MorseMode::OneShot, true) => Segment::Gap {
                        kind: GapKind::Word,
                        target: GapTarget::Continue {
                            char_offset: offset,
                            symbol_idx: 0,
                        },
                    },
                    _ => Segment::Symbol {
                        char_offset: offset,
                        symbol_idx: 0,
                    },
                };
            }
        }
    }

    /// Stops keying: a pause-to-silence, not a terminal state transition.
    ///
    /// [`output`](Self::output) returns [`KeyState::Off`] and
    /// [`update`](Self::update) becomes a no-op until [`start`](Self::start)
    /// is called again. Only the active flag is cleared —
    /// [`is_finished`](Self::is_finished) is left as-is, reflecting only
    /// natural [`MorseMode::OneShot`] completion, not `stop`. The next
    /// [`start`](Self::start) performs the full reset back to the first
    /// keyed character.
    pub fn stop(&mut self) {
        self.active = false;
    }

    /// Advances the keyer to the current tick.
    ///
    /// Returns `Some(`[`CharacterStarted`](MorseEvent::CharacterStarted)`(i))`
    /// when keying advances to a new character at byte offset `i`,
    /// `Some(`[`Finished`](MorseEvent::Finished)`)` exactly once when a
    /// [`MorseMode::OneShot`] keyer completes its message, or `None`
    /// otherwise (including whenever the keyer is inactive or already
    /// finished).
    ///
    /// `update` advances **at most one segment** (one dot, dash, or gap) and
    /// emits **at most one event** per call — see the module-level `# Clock`
    /// section for why, and how a caller catches up after falling behind.
    #[must_use]
    pub fn update(&mut self, now: u64) -> Option<MorseEvent> {
        if !self.active || self.finished {
            return None;
        }

        match self.segment {
            Segment::Idle => None,
            Segment::Symbol {
                char_offset,
                symbol_idx,
            } => self.update_symbol(now, char_offset, symbol_idx),
            Segment::Gap { kind, target } => self.update_gap(now, kind, target),
        }
    }

    /// Advances a [`Segment::Symbol`] (the `update` branch for a dot/dash).
    fn update_symbol(
        &mut self,
        now: u64,
        char_offset: usize,
        symbol_idx: usize,
    ) -> Option<MorseEvent> {
        // Defensive: `char_offset` always addresses an encodable byte by
        // construction (`start` and every `Segment` transition below only
        // ever point at bytes `code_for` recognized). The `debug_assert`
        // surfaces a broken invariant loudly in debug/test builds; the
        // `else` fallback keeps release builds panic-free if it's ever
        // wrong.
        debug_assert!(
            code_for(self.message[char_offset]).is_some(),
            "MorseKeyer::update_symbol: char_offset {char_offset} does not address an encodable byte"
        );
        let Some(code) = code_for(self.message[char_offset]) else {
            self.active = false;
            return None;
        };
        let code = code.as_bytes();
        // Defensive: `symbol_idx` never exceeds `code.len() - 1` — see above.
        debug_assert!(
            code.get(symbol_idx).is_some(),
            "MorseKeyer::update_symbol: symbol_idx {symbol_idx} out of bounds for code {code:?}"
        );
        let Some(&symbol) = code.get(symbol_idx) else {
            self.active = false;
            return None;
        };

        let duration = symbol_duration_ticks(symbol, self.unit_ticks);
        if now.saturating_sub(self.segment_since) < duration {
            return None;
        }
        self.segment_since = self.segment_since.saturating_add(duration);

        if symbol_idx + 1 < code.len() {
            self.segment = Segment::Gap {
                kind: GapKind::Intra,
                target: GapTarget::Continue {
                    char_offset,
                    symbol_idx: symbol_idx + 1,
                },
            };
            return None;
        }

        // The last symbol of this character just elapsed: find what follows.
        let (next, saw_space) = scan_next(self.message, char_offset + 1);
        match (self.mode, next) {
            (_, Some(next_offset)) => {
                self.segment = Segment::Gap {
                    kind: if saw_space {
                        GapKind::Word
                    } else {
                        GapKind::InterChar
                    },
                    target: GapTarget::CharacterStart(next_offset),
                };
                None
            }
            (MorseMode::Loop, None) => {
                // Defensive: reaching a `Symbol` segment means at least one
                // encodable character exists, so a wrap target always
                // exists.
                debug_assert!(
                    scan_next(self.message, 0).0.is_some(),
                    "MorseKeyer::update_symbol: Loop wrap found no encodable character in a keyed message"
                );
                let Some(wrap_offset) = scan_next(self.message, 0).0 else {
                    self.active = false;
                    return None;
                };
                self.segment = Segment::Gap {
                    kind: GapKind::Word,
                    target: GapTarget::CharacterStart(wrap_offset),
                };
                None
            }
            (MorseMode::OneShot, None) if saw_space => {
                self.segment = Segment::Gap {
                    kind: GapKind::Word,
                    target: GapTarget::Finished,
                };
                None
            }
            (MorseMode::OneShot, None) => {
                self.active = false;
                self.finished = true;
                Some(MorseEvent::Finished)
            }
        }
    }

    /// Advances a [`Segment::Gap`] (the `update` branch for silence).
    fn update_gap(&mut self, now: u64, kind: GapKind, target: GapTarget) -> Option<MorseEvent> {
        let duration = gap_duration_ticks(kind, self.unit_ticks);
        if now.saturating_sub(self.segment_since) < duration {
            return None;
        }
        self.segment_since = self.segment_since.saturating_add(duration);

        match target {
            GapTarget::Continue {
                char_offset,
                symbol_idx,
            } => {
                self.segment = Segment::Symbol {
                    char_offset,
                    symbol_idx,
                };
                None
            }
            GapTarget::CharacterStart(offset) => {
                self.segment = Segment::Symbol {
                    char_offset: offset,
                    symbol_idx: 0,
                };
                Some(MorseEvent::CharacterStarted(offset))
            }
            GapTarget::Finished => {
                self.active = false;
                self.finished = true;
                Some(MorseEvent::Finished)
            }
        }
    }

    /// Returns the key state the downstream adapter should currently be driving.
    ///
    /// Returns [`KeyState::Off`] when inactive, finished, or holding a gap;
    /// [`KeyState::On`] while a dot or dash is sounding.
    #[must_use]
    pub fn output(&self) -> KeyState {
        if !self.active || self.finished {
            return KeyState::Off;
        }
        match self.segment {
            Segment::Symbol { .. } => KeyState::On,
            Segment::Gap { .. } | Segment::Idle => KeyState::Off,
        }
    }

    /// Returns `true` if a [`MorseMode::OneShot`] keyer has completed.
    ///
    /// A message with no keyable content is always `true`, regardless of
    /// mode (nothing to key). For a message with keyable content,
    /// [`MorseMode::Loop`] never becomes `finished`.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Returns `true` if the keyer is currently keying (started and not yet
    /// finished or stopped).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- code_for ---

    #[test]
    fn code_for_c_and_q() {
        assert_eq!(code_for(b'C'), Some("-.-."));
        assert_eq!(code_for(b'Q'), Some("--.-"));
    }

    #[test]
    fn code_for_folds_lowercase() {
        assert_eq!(code_for(b'c'), Some("-.-."));
        assert_eq!(code_for(b'q'), Some("--.-"));
    }

    #[test]
    fn code_for_none_for_space_punctuation_and_newline() {
        assert_eq!(code_for(b' '), None);
        assert_eq!(code_for(b'!'), None);
        assert_eq!(code_for(b'.'), None); // a literal period is punctuation, not a dot symbol
        assert_eq!(code_for(b'\n'), None);
    }

    // --- Canonical CQ OneShot timeline (unit_ticks = 10) ---

    #[test]
    fn cq_oneshot_canonical_timeline() {
        let mut k = MorseKeyer::new(b"CQ", 10, MorseMode::OneShot);
        k.start(0);

        // C = "-.-." : dash[0,30) gap[30,40) dot[40,50) gap[50,60)
        //              dash[60,90) gap[90,100) dot[100,110)
        assert_eq!(k.output(), KeyState::On);

        assert_eq!(k.update(30), None); // dash ends -> intra gap
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(40), None); // gap ends -> dot
        assert_eq!(k.output(), KeyState::On);
        assert_eq!(k.update(50), None); // dot ends -> intra gap
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(60), None); // gap ends -> dash
        assert_eq!(k.output(), KeyState::On);
        assert_eq!(k.update(90), None); // dash ends -> intra gap
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(100), None); // gap ends -> dot (C's last symbol)
        assert_eq!(k.output(), KeyState::On);
        assert_eq!(k.update(110), None); // dot ends -> 3u inter-character gap
        assert_eq!(k.output(), KeyState::Off);

        // The 3-unit inter-character gap holds Off from 110 to 140.
        assert_eq!(k.update(139), None);
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(140), Some(MorseEvent::CharacterStarted(1)));
        assert_eq!(k.output(), KeyState::On);

        // Q = "--.-" : dash[140,170) gap[170,180) dash[180,210) gap[210,220)
        //              dot[220,230) gap[230,240) dash[240,270)
        assert_eq!(k.update(170), None);
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(180), None);
        assert_eq!(k.output(), KeyState::On);
        assert_eq!(k.update(210), None);
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(220), None);
        assert_eq!(k.output(), KeyState::On);
        assert_eq!(k.update(230), None);
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(240), None); // gap ends -> dash (Q's last symbol)
        assert_eq!(k.output(), KeyState::On);

        // No trailing space: Q's last symbol ends straight into Finished, no gap.
        assert_eq!(k.update(270), Some(MorseEvent::Finished));
        assert!(k.is_finished());
        assert_eq!(k.output(), KeyState::Off);

        // Subsequent updates are inert.
        assert_eq!(k.update(1_000), None);
        assert_eq!(k.output(), KeyState::Off);
    }

    #[test]
    fn cq_loop_continuation_never_finishes() {
        let mut k = MorseKeyer::new(b"CQ", 10, MorseMode::Loop);
        k.start(0);

        // Walk to the end of Q exactly as the OneShot timeline does.
        for t in [30, 40, 50, 60, 90, 100, 110] {
            assert_eq!(k.update(t), None);
        }
        assert_eq!(k.update(140), Some(MorseEvent::CharacterStarted(1)));
        for t in [170, 180, 210, 220, 230, 240] {
            assert_eq!(k.update(t), None);
        }

        // Q's last symbol ends at 270; under `Loop` this enters the 7-unit
        // word-gap seam instead of finishing.
        assert_eq!(k.update(270), None);
        assert!(!k.is_finished());
        assert_eq!(k.output(), KeyState::Off);

        // The seam elapses at 270 + 70 = 340, wrapping to C (byte offset 0).
        assert_eq!(k.update(339), None);
        assert_eq!(k.update(340), Some(MorseEvent::CharacterStarted(0)));
        assert_eq!(k.output(), KeyState::On);
        assert!(!k.is_finished());
        assert!(k.is_active());
    }

    // --- Gap sizes ---

    #[test]
    fn intra_inter_and_word_gap_durations() {
        // "K" = "-.-" isolates an intra-character gap (1u).
        let mut k = MorseKeyer::new(b"K", 10, MorseMode::OneShot);
        k.start(0);
        assert_eq!(k.update(29), None); // dash[0,30) not yet elapsed
        assert_eq!(k.update(30), None); // dash ends -> intra gap
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(39), None); // 1u gap not yet elapsed
        assert_eq!(k.update(40), None); // gap ends -> dot

        // "EE" isolates an inter-character gap (3u) between two single-dot letters.
        let mut k = MorseKeyer::new(b"EE", 10, MorseMode::OneShot);
        k.start(0);
        assert_eq!(k.update(10), None); // first E's dot ends -> inter-char gap
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(39), None); // 3u gap not yet elapsed
        assert_eq!(k.update(40), Some(MorseEvent::CharacterStarted(1)));

        // "E E" isolates a word gap (7u) between two single-dot letters.
        let mut k = MorseKeyer::new(b"E E", 10, MorseMode::OneShot);
        k.start(0);
        assert_eq!(k.update(10), None); // first E's dot ends -> word gap
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(79), None); // 7u gap not yet elapsed
        assert_eq!(k.update(80), Some(MorseEvent::CharacterStarted(2)));
    }

    #[test]
    fn consecutive_spaces_collapse_to_one_word_gap() {
        let mut k = MorseKeyer::new(b"A  B", 10, MorseMode::OneShot);
        k.start(0);
        // A = ".-" : dot[0,10) gap[10,20) dash[20,50)
        assert_eq!(k.update(10), None);
        assert_eq!(k.update(20), None);
        assert_eq!(k.update(50), None); // A's dash ends -> a single 7u word gap
        assert_eq!(k.update(119), None); // 7u == 70 ticks: 50 + 70 = 120
        assert_eq!(k.update(120), Some(MorseEvent::CharacterStarted(3))); // B's byte offset
    }

    // --- Skip-byte transparency ---

    #[test]
    fn skip_bytes_are_transparent_and_tick_free() {
        let mut with_skip = MorseKeyer::new(b"C!Q", 10, MorseMode::OneShot);
        let mut without_skip = MorseKeyer::new(b"CQ", 10, MorseMode::OneShot);
        with_skip.start(0);
        without_skip.start(0);

        for t in [
            30, 40, 50, 60, 90, 100, 110, 140, 170, 180, 210, 220, 230, 240, 270,
        ] {
            let a = with_skip.update(t);
            let b = without_skip.update(t);
            // `b"C!Q"`'s Q sits at byte offset 2, not 1, but timing is identical.
            match (a, b) {
                (
                    Some(MorseEvent::CharacterStarted(ia)),
                    Some(MorseEvent::CharacterStarted(ib)),
                ) => {
                    assert_eq!(ia, 2);
                    assert_eq!(ib, 1);
                }
                (other_a, other_b) => assert_eq!(other_a, other_b),
            }
        }
        assert!(with_skip.is_finished());
        assert!(without_skip.is_finished());
    }

    // --- Byte-offset events under skipping ---

    #[test]
    fn character_started_reports_byte_offsets_not_a_character_count() {
        // "!A B": '!' at 0 is skipped, A at byte offset 1, space at 2, B at
        // byte offset 3. A is the first keyed character (loaded by `start`,
        // no leading space since '!' isn't a space), so only B's start fires.
        let mut k = MorseKeyer::new(b"!A B", 10, MorseMode::OneShot);
        k.start(0);
        assert_eq!(k.output(), KeyState::On); // A's dot, byte offset 1

        // A = ".-" : dot[0,10) gap[10,20) dash[20,50) -> word gap (space at byte 2)
        assert_eq!(k.update(10), None);
        assert_eq!(k.update(20), None);
        assert_eq!(k.update(50), None); // A's dash ends -> 7u word gap
        assert_eq!(k.update(120), Some(MorseEvent::CharacterStarted(3))); // B, not 1
    }

    #[test]
    fn leading_delay_gap_to_first_character_fires_no_event() {
        // A literal leading space (not just a skip-byte) still delays the
        // first symbol without firing `CharacterStarted` for it: the first
        // keyed character is "loaded" by `start`, even when a gap defers its
        // audible start.
        let mut k = MorseKeyer::new(b" A B", 10, MorseMode::OneShot);
        k.start(0);
        assert_eq!(k.output(), KeyState::Off); // leading word gap, not yet keying

        assert_eq!(k.update(69), None);
        assert_eq!(k.update(70), None); // gap ends -> A's dot, silently (no event)
        assert_eq!(k.output(), KeyState::On);

        // A = ".-" starting at t=70: dot[70,80) gap[80,90) dash[90,120) -> word gap
        assert_eq!(k.update(80), None);
        assert_eq!(k.update(90), None);
        assert_eq!(k.update(120), None);
        assert_eq!(k.update(190), Some(MorseEvent::CharacterStarted(3))); // B, first real event
    }

    // --- Bounded catch-up under a late poll ---

    #[test]
    fn late_poll_advances_exactly_one_segment_per_call() {
        let mut k = MorseKeyer::new(b"CQ", 10, MorseMode::OneShot);
        k.start(0);

        // Poll far past every boundary with the same `now`: each call still
        // advances only one segment, so it takes many calls to reach the
        // first reported event (7 silent segments through C, then the
        // inter-character gap fires it on the 8th call).
        for _ in 0..7 {
            assert_eq!(k.update(10_000), None);
        }
        assert_eq!(k.update(10_000), Some(MorseEvent::CharacterStarted(1)));

        // Likewise for Q -> Finished: 6 silent segments, then the 7th call
        // (Q's last symbol boundary) reports `Finished` directly.
        for _ in 0..6 {
            assert_eq!(k.update(10_000), None);
        }
        assert_eq!(k.update(10_000), Some(MorseEvent::Finished));
    }

    // --- Leading/trailing space (OneShot honors them literally) ---

    #[test]
    fn leading_space_delays_first_oneshot_symbol_by_one_word_gap() {
        let mut k = MorseKeyer::new(b" E", 10, MorseMode::OneShot);
        k.start(0);
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(69), None);
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(70), None); // 7u gap elapses -> E's dot, no event
        assert_eq!(k.output(), KeyState::On);
    }

    #[test]
    fn trailing_space_adds_word_gap_before_finished() {
        let mut k = MorseKeyer::new(b"E ", 10, MorseMode::OneShot);
        k.start(0);
        assert_eq!(k.update(10), None); // E's dot ends -> 7u trailing word gap
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(79), None);
        assert_eq!(k.update(80), Some(MorseEvent::Finished));
        assert!(k.is_finished());
    }

    // --- Loop seams collapse edge spaces, and Loop begins immediately ---

    #[test]
    fn loop_seam_collapses_trailing_space_to_one_word_gap() {
        let mut k = MorseKeyer::new(b"CQ ", 10, MorseMode::Loop);
        k.start(0);
        assert_eq!(k.output(), KeyState::On); // begins keying immediately

        for t in [30, 40, 50, 60, 90, 100, 110] {
            assert_eq!(k.update(t), None);
        }
        assert_eq!(k.update(140), Some(MorseEvent::CharacterStarted(1)));
        for t in [170, 180, 210, 220, 230, 240] {
            assert_eq!(k.update(t), None);
        }
        assert_eq!(k.update(270), None); // Q ends -> exactly one 7u seam gap
        assert_eq!(k.update(340), Some(MorseEvent::CharacterStarted(0)));
        assert!(!k.is_finished());
    }

    #[test]
    fn loop_seam_collapses_leading_space_with_no_leading_gap() {
        let mut k = MorseKeyer::new(b" CQ", 10, MorseMode::Loop);
        k.start(0);
        // C is at byte offset 1; Loop ignores the leading space entirely.
        assert_eq!(k.output(), KeyState::On);

        for t in [30, 40, 50, 60, 90, 100, 110] {
            assert_eq!(k.update(t), None);
        }
        assert_eq!(k.update(140), Some(MorseEvent::CharacterStarted(2))); // Q at byte offset 2
        for t in [170, 180, 210, 220, 230, 240] {
            assert_eq!(k.update(t), None);
        }
        assert_eq!(k.update(270), None); // seam gap begins
        assert_eq!(k.update(340), Some(MorseEvent::CharacterStarted(1))); // wraps to C at offset 1
    }

    #[test]
    fn loop_seam_collapses_both_edge_spaces_to_one_word_gap() {
        let mut k = MorseKeyer::new(b" CQ ", 10, MorseMode::Loop);
        k.start(0);
        assert_eq!(k.output(), KeyState::On); // no leading gap

        for t in [30, 40, 50, 60, 90, 100, 110] {
            assert_eq!(k.update(t), None);
        }
        assert_eq!(k.update(140), Some(MorseEvent::CharacterStarted(2)));
        for t in [170, 180, 210, 220, 230, 240] {
            assert_eq!(k.update(t), None);
        }
        // Both the trailing space and the seam collapse into one 7u gap.
        assert_eq!(k.update(270), None);
        assert_eq!(k.update(340), Some(MorseEvent::CharacterStarted(1)));
    }

    // --- Content shapes ---

    #[test]
    fn empty_message_is_finished_immediately_and_never_panics() {
        let mut k = MorseKeyer::new(b"", 10, MorseMode::OneShot);
        assert!(k.is_finished());
        assert_eq!(k.output(), KeyState::Off);

        k.start(0);
        assert!(k.is_finished());
        assert!(!k.is_active());
        assert_eq!(k.update(0), None);
        assert_eq!(k.output(), KeyState::Off);
    }

    #[test]
    fn all_unsupported_message_is_finished_immediately() {
        let mut k = MorseKeyer::new(b"!!!", 10, MorseMode::Loop);
        assert!(k.is_finished());
        k.start(0);
        assert!(k.is_finished());
        assert!(!k.is_active());
        assert_eq!(k.update(0), None);
        assert_eq!(k.output(), KeyState::Off);
    }

    #[test]
    fn all_space_oneshot_fires_one_finished_after_one_word_gap() {
        let mut k = MorseKeyer::new(b"   ", 10, MorseMode::OneShot);
        assert!(!k.is_finished()); // has timed content, just no character
        k.start(0);
        assert!(k.is_active());
        assert!(!k.is_finished());
        assert_eq!(k.output(), KeyState::Off);

        assert_eq!(k.update(69), None);
        assert_eq!(k.update(70), Some(MorseEvent::Finished));
        assert!(k.is_finished());
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(1_000), None); // inert afterward
    }

    #[test]
    fn all_space_loop_emits_zero_events_and_stays_off() {
        let mut k = MorseKeyer::new(b"   ", 10, MorseMode::Loop);
        k.start(0);
        assert!(k.is_active());
        assert!(!k.is_finished());
        assert_eq!(k.output(), KeyState::Off);

        for t in [0, 10, 1_000, 1_000_000, u64::MAX] {
            assert_eq!(k.update(t), None);
            assert_eq!(k.output(), KeyState::Off);
        }
        assert!(!k.is_finished());
    }

    // --- unit_ticks == 0 ---

    #[test]
    fn zero_unit_ticks_advances_one_segment_per_update_without_panic() {
        let mut k = MorseKeyer::new(b"CQ", 0, MorseMode::OneShot);
        k.start(0);

        // C (4 symbols + 3 intra gaps) then the inter-character gap: 8 total
        // zero-duration segments before `CharacterStarted(1)` fires.
        for _ in 0..7 {
            assert_eq!(k.update(0), None);
        }
        assert_eq!(k.update(0), Some(MorseEvent::CharacterStarted(1)));

        // Q (4 symbols + 3 intra gaps): the 7th call's own boundary reports
        // `Finished` directly (no trailing space, no separate gap segment).
        for _ in 0..6 {
            assert_eq!(k.update(0), None);
        }
        assert_eq!(k.update(0), Some(MorseEvent::Finished));
        assert!(k.is_finished());
    }

    // --- Non-monotonic / near-u64::MAX ---

    #[test]
    fn non_monotonic_now_saturates_without_panic_or_spurious_advance() {
        let mut k = MorseKeyer::new(b"E", 10, MorseMode::OneShot);
        k.start(100);

        // `now` earlier than the segment's start: saturating_sub clamps to 0.
        assert_eq!(k.update(50), None);
        assert_eq!(k.output(), KeyState::On);
        // Still doesn't advance until the true boundary from segment_since=100.
        assert_eq!(k.update(109), None);
        assert_eq!(k.update(110), Some(MorseEvent::Finished));
    }

    #[test]
    fn near_u64_max_start_and_update_never_panic() {
        let mut k = MorseKeyer::new(b"E", 10, MorseMode::OneShot);
        let start = u64::MAX - 10;
        k.start(start);
        assert_eq!(k.update(u64::MAX - 1), None);
        assert_eq!(k.update(u64::MAX), Some(MorseEvent::Finished));
        assert!(k.is_finished());
    }

    #[test]
    fn huge_unit_ticks_never_panics_on_duration_multiplication() {
        // `DASH_UNITS * unit_ticks` would overflow-panic on a naive `*`;
        // `saturating_mul` must clamp instead. Completion is not asserted —
        // rollover/saturation behavior is explicitly out of contract.
        let mut k = MorseKeyer::new(b"CQ", u64::MAX / 2, MorseMode::OneShot);
        k.start(0);
        let _ = k.update(u64::MAX);
        let _ = k.output();
    }

    // --- start() restarts, including a leading-gap first segment ---

    #[test]
    fn start_restarts_from_the_first_keyed_character_and_resets_baseline() {
        let mut k = MorseKeyer::new(b"CQ", 10, MorseMode::OneShot);
        k.start(0);
        assert_eq!(k.update(140), None); // not yet caught up to the real event

        // Restart mid-sequence: back to C, baseline reset to the new `now`.
        k.start(1_000);
        assert_eq!(k.output(), KeyState::On);
        assert!(k.is_active());
        assert!(!k.is_finished());
        assert_eq!(k.update(1_029), None);
        assert_eq!(k.update(1_030), None); // C's dash ends on the new schedule
    }

    #[test]
    fn start_restart_with_a_leading_word_gap_as_the_first_segment() {
        let mut k = MorseKeyer::new(b" E", 10, MorseMode::OneShot);
        k.start(0);
        assert_eq!(k.update(70), None); // consumes the leading gap, no event

        // Restart: the leading gap reappears as the first segment again.
        k.start(500);
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(569), None);
        assert_eq!(k.update(570), None); // gap elapses -> E's dot, silently
        assert_eq!(k.output(), KeyState::On);
    }

    #[test]
    fn start_after_finished_resets_finished_flag() {
        let mut k = MorseKeyer::new(b"E", 10, MorseMode::OneShot);
        k.start(0);
        assert_eq!(k.update(10), Some(MorseEvent::Finished));
        assert!(k.is_finished());

        k.start(100);
        assert!(!k.is_finished());
        assert!(k.is_active());
        assert_eq!(k.output(), KeyState::On);
    }

    // --- stop() ---

    #[test]
    fn stop_silences_output_suspends_update_and_leaves_is_finished_unchanged() {
        let mut k = MorseKeyer::new(b"E", 10, MorseMode::Loop);
        k.start(0);
        assert!(k.is_active());
        assert!(!k.is_finished());

        k.stop();
        assert!(!k.is_active());
        assert!(!k.is_finished()); // unchanged: `stop` never finishes anything
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(1_000), None); // no-op while stopped
        assert_eq!(k.output(), KeyState::Off);
    }

    // --- inactive before start ---

    #[test]
    fn inactive_before_start_is_silent_and_update_is_noop() {
        let mut k = MorseKeyer::new(b"CQ", 10, MorseMode::OneShot);
        assert!(!k.is_active());
        assert!(!k.is_finished());
        assert_eq!(k.output(), KeyState::Off);
        assert_eq!(k.update(0), None);
        assert_eq!(k.update(1_000), None);
    }
}
