//! Infrared pulse codec — [`PulseCodec`], [`NecFrame`], and the sans-io [`FrameAssembler`].
//!
//! Consumer IR remotes speak pulse-distance or pulse-width protocols that are
//! rarely textbook NEC, so the codec is *data*: header, bit-0 and bit-1
//! timings, trailer, bit count, bit order, and a decode tolerance.
//! Standard NEC is one preset, [`PulseCodec::nec`].
//! Device timings and button tables stay downstream.
//!
//! # Pulse model
//!
//! A pulse list is a `&[u32]` of alternating mark, space, mark, ... durations
//! in microseconds.
//! Index 0 is always a mark.
//! An odd length is normal: a frame ends on its trailer (or last) mark, and the
//! idle gap after it is never part of the list.
//! [`PulseBuf`] is the fixed-capacity, alloc-free container for encode output.
//!
//! # Seam
//!
//! This module is HAL-agnostic and imports nothing outside `tamer`.
//! A hardware tier produces edges (or RMT segments) and consumes pulse lists
//! (see [`pulse_pairs()`](crate::ir::pulse_pairs)); the edge and pulse feed itself is the test seam.
//! For firmware that holds a transmitter or receiver, [`PulseSink`] and
//! [`PulseSource`] abstract the bridge, with [`MockPulseSink`] and
//! [`MockPulseSource`] as the host-test doubles.
//!
//! Only the headline types are re-exported at the `tamer` root; the error and
//! parameter types (`Tolerance`, `BitOrder`, `DecodeError`, ...) and the free
//! functions (`carrier_ticks`, `pulse_pairs`, `validate_pulses`,
//! `is_nec_repeat`, `NEC_REPEAT`) stay under `tamer::ir::`.
//!
//! # Example
//!
//! ```
//! use tamer::ir::{NecFrame, PulseBuf, PulseCodec};
//!
//! let codec = PulseCodec::nec();
//! let frame = NecFrame::new(0x20, 0x0B);
//! let pulses: PulseBuf<67> = codec.encode(frame.to_word()).unwrap();
//! assert_eq!(pulses.len(), 67);
//!
//! let word = codec.decode(pulses.as_slice()).unwrap();
//! assert_eq!(NecFrame::from_word(word), Ok(frame));
//! ```

use core::fmt;

// ---------------------------------------------------------------------------
// Pulse model
// ---------------------------------------------------------------------------

/// Fixed-capacity pulse list (no allocation); the encode output container.
///
/// Holds up to `N` durations in microseconds, alternating mark and space,
/// starting with a mark.
/// `PulseBuf<67>` (`2 + 64 + 1`) fits every codec, a NEC frame included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PulseBuf<const N: usize> {
    buf: [u32; N],
    len: usize,
}

impl<const N: usize> PulseBuf<N> {
    /// Create an empty buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }

    /// Number of pulses stored.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// `true` when no pulse is stored.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Capacity `N`.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// The stored pulses, mark first.
    #[must_use]
    pub fn as_slice(&self) -> &[u32] {
        &self.buf[..self.len]
    }

    /// Append one duration in microseconds.
    ///
    /// # Errors
    ///
    /// [`EncodeError::BufferTooSmall`] when the buffer is full.
    pub fn push(&mut self, us: u32) -> Result<(), EncodeError> {
        match self.buf.get_mut(self.len) {
            Some(slot) => {
                *slot = us;
                self.len += 1;
                Ok(())
            }
            None => Err(EncodeError::BufferTooSmall {
                needed: self.len + 1,
            }),
        }
    }

    /// Remove all pulses.
    pub fn clear(&mut self) {
        self.len = 0;
    }
}

impl<const N: usize> Default for PulseBuf<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Iterate a pulse list as `(mark, space)` pairs.
///
/// A trailing lone mark yields `(mark, 0)`.
/// This lets a transmitter build RMT-style entries without re-deriving the
/// alternation.
///
/// ```
/// use tamer::ir::pulse_pairs;
///
/// let mut it = pulse_pairs(&[9000, 4500, 560]);
/// assert_eq!(it.next(), Some((9000, 4500)));
/// assert_eq!(it.next(), Some((560, 0)));
/// assert_eq!(it.next(), None);
/// ```
pub fn pulse_pairs(pulses: &[u32]) -> impl Iterator<Item = (u32, u32)> + '_ {
    pulses.chunks(2).map(|c| match *c {
        [mark, space] => (mark, space),
        [mark] => (mark, 0),
        _ => (0, 0),
    })
}

/// Why a pulse list cannot be sent by a hardware transmitter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PulseListError {
    /// The list is empty.
    Empty,
    /// The pulse at `index` is zero; hardware reads a zero length as the end.
    Zero {
        /// Position in the list, from 0.
        index: usize,
    },
    /// The pulse at `index` exceeds the hardware limit.
    TooLong {
        /// Position in the list, from 0.
        index: usize,
    },
}

/// Check a pulse list for a transmitter whose entries are `1..=max_us` ticks.
///
/// Rejects an empty list, a zero pulse (it would end the transmission early)
/// and a pulse above `max_us` (for the RMT, 32767).
///
/// ```
/// use tamer::ir::{validate_pulses, PulseListError};
///
/// assert_eq!(validate_pulses(&[9000, 4500, 560], 32_767), Ok(()));
/// assert_eq!(validate_pulses(&[], 32_767), Err(PulseListError::Empty));
/// assert_eq!(validate_pulses(&[9000, 0], 32_767), Err(PulseListError::Zero { index: 1 }));
/// ```
///
/// # Errors
///
/// [`PulseListError`] naming the first offending pulse.
pub fn validate_pulses(pulses: &[u32], max_us: u32) -> Result<(), PulseListError> {
    if pulses.is_empty() {
        return Err(PulseListError::Empty);
    }
    for (index, &p) in pulses.iter().enumerate() {
        if p == 0 {
            return Err(PulseListError::Zero { index });
        }
        if p > max_us {
            return Err(PulseListError::TooLong { index });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why a [`PulseCodec`] could not be constructed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CodecError {
    /// The bit count is not in `1..=32`.
    InvalidBitCount,
    /// The tolerance windows of the zero and one bit overlap in both mark and
    /// space, so decode could not tell them apart.
    OverlappingBits,
}

/// Why a value could not be encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodeError {
    /// The value has bits set above the codec's bit count.
    ValueTooWide,
    /// The output buffer holds fewer than `needed` pulses.
    BufferTooSmall {
        /// Pulses required.
        needed: usize,
    },
}

/// Why a pulse list could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The pulse list is empty.
    Empty,
    /// The header mark is too long, the header space is off, or the header
    /// space is missing.
    HeaderMismatch,
    /// The pulse count does not fit the codec.
    WrongBitCount {
        /// Bits the codec expects.
        expected: u8,
        /// Bit pairs found in the list (complete or partial).
        found: usize,
    },
    /// The bit at `index` (transmission order) matches neither zero nor one.
    BitMismatch {
        /// Bit position in transmission order, from 0.
        index: u8,
    },
    /// The trailer mark is off.
    TrailerMismatch,
}

/// Why a NEC data word failed its inverted-byte check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum NecError {
    /// An inverted byte is not the complement of its partner.
    InvertedByteMismatch,
}

/// Why a captured frame was not usable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FrameError {
    /// More pulses arrived than the assembler holds; the excess was dropped.
    Overflow,
}

macro_rules! impl_error {
    ($ty:ty, $($pat:pat => $($arg:expr),+ ;)+) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match *self {
                    $($pat => write!(f, $($arg),+),)+
                }
            }
        }
        impl core::error::Error for $ty {}
    };
}

impl_error!(CodecError,
    Self::InvalidBitCount => "bit count must be 1..=32";
    Self::OverlappingBits => "zero and one bit timings overlap";
);
impl_error!(EncodeError,
    Self::ValueTooWide => "value has bits above the codec bit count";
    Self::BufferTooSmall { needed } => "output buffer too small, {} pulses needed", needed;
);
impl_error!(DecodeError,
    Self::Empty => "empty pulse list";
    Self::HeaderMismatch => "header mismatch";
    Self::WrongBitCount { expected, found } => "expected {} bits, found {}", expected, found;
    Self::BitMismatch { index } => "bit {} matches neither zero nor one", index;
    Self::TrailerMismatch => "trailer mismatch";
);
impl_error!(NecError,
    Self::InvertedByteMismatch => "inverted byte mismatch";
);
impl_error!(PulseListError,
    Self::Empty => "empty pulse list";
    Self::Zero { index } => "pulse {} is zero", index;
    Self::TooLong { index } => "pulse {} is too long", index;
);
impl_error!(FrameError,
    Self::Overflow => "frame exceeded the assembler capacity";
);

// ---------------------------------------------------------------------------
// Codec as data
// ---------------------------------------------------------------------------

/// A mark and a space duration in microseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkSpace {
    /// Mark (carrier on) duration in microseconds.
    pub mark: u32,
    /// Space (carrier off) duration in microseconds.
    pub space: u32,
}

impl MarkSpace {
    /// Create a mark/space pair.
    #[must_use]
    pub const fn new(mark: u32, space: u32) -> Self {
        Self { mark, space }
    }
}

/// Which end of the data word is transmitted first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitOrder {
    /// Bit 0 of the value goes first.
    LsbFirst,
    /// The most significant of the codec's bits goes first.
    MsbFirst,
}

/// Symmetric decode tolerance, inclusive at the edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tolerance {
    /// A percentage of the nominal duration (saturates at 100).
    Percent(u8),
    /// A fixed number of microseconds.
    Micros(u32),
}

impl Tolerance {
    /// Allowed deviation in whole microseconds, rounded up so the overlap check
    /// is conservative (it may reject a codec by one microsecond, never accept
    /// an ambiguous one); only for validation.
    const fn window(self, nominal: u32) -> (u64, u64) {
        let nominal = nominal as u64;
        let d = match self {
            Self::Percent(p) => {
                let p = if p > 100 { 100 } else { p };
                (nominal * p as u64).div_ceil(100)
            }
            Self::Micros(m) => m as u64,
        };
        (nominal.saturating_sub(d), nominal + d)
    }

    /// `true` when `|actual - nominal|` is within the tolerance (inclusive).
    #[must_use]
    pub const fn matches(self, nominal: u32, actual: u32) -> bool {
        let diff = actual.abs_diff(nominal) as u64;
        self.allows(nominal, diff)
    }

    /// `true` when `actual <= nominal + tolerance`: a one-sided check used for
    /// a first mark that a late-armed receiver may have truncated.
    #[must_use]
    pub const fn at_most(self, nominal: u32, actual: u32) -> bool {
        actual <= nominal || self.allows(nominal, (actual - nominal) as u64)
    }

    // Division-free: `diff * 100 <= nominal * pct` (u64 cannot overflow).
    const fn allows(self, nominal: u32, diff: u64) -> bool {
        match self {
            Self::Percent(p) => {
                let p = if p > 100 { 100 } else { p };
                diff * 100 <= nominal as u64 * p as u64
            }
            Self::Micros(m) => diff <= m as u64,
        }
    }
}

const fn windows_overlap(tol: Tolerance, a: u32, b: u32) -> bool {
    let (alo, ahi) = tol.window(a);
    let (blo, bhi) = tol.window(b);
    alo <= bhi && blo <= ahi
}

/// An IR pulse protocol described as data.
///
/// Build one with [`PulseCodec::new`] or take the [`PulseCodec::nec`] preset.
/// The decoded value is the natural integer in either bit order; see
/// [`BitOrder`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PulseCodec {
    header: Option<MarkSpace>,
    zero: MarkSpace,
    one: MarkSpace,
    trailer: Option<u32>,
    bits: u8,
    order: BitOrder,
    tolerance: Tolerance,
}

impl PulseCodec {
    /// Create a codec.
    ///
    /// # Errors
    ///
    /// [`CodecError::InvalidBitCount`] unless `bits` is in `1..=32`.
    /// [`CodecError::OverlappingBits`] if the tolerance windows of `zero` and
    /// `one` overlap in both mark and space.
    #[allow(clippy::too_many_arguments)] // positional by project convention
    pub const fn new(
        header: Option<MarkSpace>,
        zero: MarkSpace,
        one: MarkSpace,
        trailer: Option<u32>,
        bits: u8,
        order: BitOrder,
        tolerance: Tolerance,
    ) -> Result<Self, CodecError> {
        if bits == 0 || bits > 32 {
            return Err(CodecError::InvalidBitCount);
        }
        if windows_overlap(tolerance, zero.mark, one.mark)
            && windows_overlap(tolerance, zero.space, one.space)
        {
            return Err(CodecError::OverlappingBits);
        }
        Ok(Self {
            header,
            zero,
            one,
            trailer,
            bits,
            order,
            tolerance,
        })
    }

    /// Standard NEC: header 9000/4500, zero 560/560, one 560/1690, trailer 560,
    /// 32 bits, LSB first, 25 % tolerance.
    #[must_use]
    pub const fn nec() -> Self {
        Self {
            header: Some(MarkSpace::new(9000, 4500)),
            zero: MarkSpace::new(560, 560),
            one: MarkSpace::new(560, 1690),
            trailer: Some(560),
            bits: 32,
            order: BitOrder::LsbFirst,
            tolerance: Tolerance::Percent(25),
        }
    }

    /// Number of data bits.
    #[must_use]
    pub const fn bits(&self) -> u8 {
        self.bits
    }

    /// Bit order on the wire.
    #[must_use]
    pub const fn bit_order(&self) -> BitOrder {
        self.order
    }

    /// Exact pulse count [`encode`](Self::encode) produces:
    /// `2 * header + 2 * bits + trailer`.
    #[must_use]
    pub const fn pulse_len(&self) -> usize {
        let h = if self.header.is_some() { 2 } else { 0 };
        let t = if self.trailer.is_some() { 1 } else { 0 };
        h + 2 * self.bits as usize + t
    }

    /// Write the pulses for `value` into `out` and return how many were written.
    ///
    /// # Errors
    ///
    /// [`EncodeError::ValueTooWide`] if `value` has bits set above the bit count.
    /// [`EncodeError::BufferTooSmall`] if `out` is shorter than [`pulse_len`](Self::pulse_len).
    pub fn encode_into(&self, value: u32, out: &mut [u32]) -> Result<usize, EncodeError> {
        if self.bits < 32 && value >> self.bits != 0 {
            return Err(EncodeError::ValueTooWide);
        }
        let needed = self.pulse_len();
        let out = out
            .get_mut(..needed)
            .ok_or(EncodeError::BufferTooSmall { needed })?;
        let mut slots = out.iter_mut();
        let mut put = |us: u32| {
            if let Some(slot) = slots.next() {
                *slot = us;
            }
        };
        if let Some(h) = self.header {
            put(h.mark);
            put(h.space);
        }
        for i in 0..u32::from(self.bits) {
            let shift = match self.order {
                BitOrder::LsbFirst => i,
                BitOrder::MsbFirst => u32::from(self.bits) - 1 - i,
            };
            let bit = if (value >> shift) & 1 == 1 {
                self.one
            } else {
                self.zero
            };
            put(bit.mark);
            put(bit.space);
        }
        if let Some(t) = self.trailer {
            put(t);
        }
        Ok(needed)
    }

    /// Encode `value` into a fixed-capacity buffer.
    ///
    /// `N = 67` covers every possible codec.
    ///
    /// # Errors
    ///
    /// As [`encode_into`](Self::encode_into); `N` is the buffer size.
    pub fn encode<const N: usize>(&self, value: u32) -> Result<PulseBuf<N>, EncodeError> {
        let mut buf = PulseBuf::<N>::new();
        buf.len = self.encode_into(value, &mut buf.buf)?;
        Ok(buf)
    }

    /// Decode a pulse list to its data word.
    ///
    /// With a header, the first mark need only be at most the nominal plus
    /// tolerance (a late-armed receiver may have cut it short, down to 0 µs);
    /// the header space is the real anchor and must match.
    /// A headerless codec checks its first mark like any other (limitation).
    /// The pulse count is checked before any bit is classified.
    /// A codec without a trailer may omit the very last space (it is the idle
    /// gap); that bit is then classified by its mark alone.
    ///
    /// # Errors
    ///
    /// [`DecodeError::Empty`] for an empty list;
    /// [`DecodeError::HeaderMismatch`] for a bad header;
    /// [`DecodeError::WrongBitCount`] for a wrong pulse count;
    /// [`DecodeError::BitMismatch`] for a bit that is neither zero nor one;
    /// [`DecodeError::TrailerMismatch`] for a bad trailer mark.
    pub fn decode(&self, pulses: &[u32]) -> Result<u32, DecodeError> {
        if pulses.is_empty() {
            return Err(DecodeError::Empty);
        }
        let body = match self.header {
            Some(h) => match *pulses {
                [mark, space, ref rest @ ..]
                    if self.tolerance.at_most(h.mark, mark)
                        && self.tolerance.matches(h.space, space) =>
                {
                    rest
                }
                _ => return Err(DecodeError::HeaderMismatch),
            },
            None => pulses,
        };

        let bits = usize::from(self.bits);
        let trailer = usize::from(self.trailer.is_some());
        let len_ok =
            body.len() == 2 * bits + trailer || (trailer == 0 && body.len() + 1 == 2 * bits);
        if !len_ok {
            return Err(DecodeError::WrongBitCount {
                expected: self.bits,
                found: body.len().saturating_sub(trailer) / 2,
            });
        }

        let tol = self.tolerance;
        let mut value = 0u32;
        for i in 0..bits {
            let mark = body.get(2 * i).copied().unwrap_or(0);
            let space = body.get(2 * i + 1).copied();
            let fits = |t: MarkSpace| {
                tol.matches(t.mark, mark) && space.is_none_or(|s| tol.matches(t.space, s))
            };
            let bit = match (fits(self.zero), fits(self.one)) {
                (true, false) => 0,
                (false, true) => 1,
                _ => {
                    return Err(DecodeError::BitMismatch {
                        index: i as u8, // i < 32
                    });
                }
            };
            let shift = match self.order {
                BitOrder::LsbFirst => i,
                BitOrder::MsbFirst => bits - 1 - i,
            };
            value |= bit << shift;
        }

        if let Some(t) = self.trailer {
            let mark = body.get(2 * bits).copied().unwrap_or(0);
            if !tol.matches(t, mark) {
                return Err(DecodeError::TrailerMismatch);
            }
        }
        Ok(value)
    }
}

// ---------------------------------------------------------------------------
// NEC helpers
// ---------------------------------------------------------------------------

/// A standard NEC frame: 8-bit address and 8-bit command.
///
/// Use with [`PulseCodec::nec`]: [`to_word`](Self::to_word) feeds
/// [`PulseCodec::encode`], and [`from_word`](Self::from_word) checks the output
/// of [`PulseCodec::decode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NecFrame {
    /// Device address.
    pub address: u8,
    /// Command code.
    pub command: u8,
}

impl NecFrame {
    /// Create a frame.
    #[must_use]
    pub const fn new(address: u8, command: u8) -> Self {
        Self { address, command }
    }

    /// The 32-bit wire word: `address | !address << 8 | command << 16 | !command << 24`.
    #[must_use]
    pub const fn to_word(self) -> u32 {
        self.address as u32
            | (!self.address as u32) << 8
            | (self.command as u32) << 16
            | (!self.command as u32) << 24
    }

    /// Parse a word, verifying both inverted bytes.
    ///
    /// # Errors
    ///
    /// [`NecError::InvertedByteMismatch`] if either inverse is wrong.
    pub const fn from_word(word: u32) -> Result<Self, NecError> {
        if (word ^ (word >> 8)) & 0xFF != 0xFF {
            return Err(NecError::InvertedByteMismatch);
        }
        match Self::from_word_extended(word) {
            Ok((address, command)) => Ok(Self {
                address: address as u8, // low byte
                command,
            }),
            Err(e) => Err(e),
        }
    }

    /// Parse an extended-NEC word: a 16-bit address with no address inversion;
    /// only the command inversion is checked.
    ///
    /// # Errors
    ///
    /// [`NecError::InvertedByteMismatch`] if the command inverse is wrong.
    pub const fn from_word_extended(word: u32) -> Result<(u16, u8), NecError> {
        if ((word >> 16) ^ (word >> 24)) & 0xFF != 0xFF {
            return Err(NecError::InvertedByteMismatch);
        }
        Ok(((word & 0xFFFF) as u16, ((word >> 16) & 0xFF) as u8))
    }
}

/// The NEC repeat frame: 9000 µs mark, 2250 µs space, 560 µs mark.
pub const NEC_REPEAT: [u32; 3] = [9000, 2250, 560];

/// `true` if `pulses` is a NEC repeat frame.
///
/// The first mark need only be at most nominal plus 25 % (it may be truncated);
/// the rest must be within 25 %.
/// The tolerance is fixed at 25 % and does not follow a tuned [`PulseCodec`].
/// The caller owns the roughly 110 ms repeat cadence; there is no repeat state.
#[must_use]
pub fn is_nec_repeat(pulses: &[u32]) -> bool {
    let tol = Tolerance::Percent(25);
    match *pulses {
        [mark, space, trailer] => {
            tol.at_most(NEC_REPEAT[0], mark)
                && tol.matches(NEC_REPEAT[1], space)
                && tol.matches(NEC_REPEAT[2], trailer)
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Carrier timing
// ---------------------------------------------------------------------------

/// Carrier half-periods in clock ticks, for a hardware modulator that counts
/// a high phase and a low phase (such as the ESP32 RMT TX carrier).
///
/// Returns `(high, low)` for a carrier of `freq_hz` and `duty_percent`
/// (share of the period spent high), counted in a clock of `clock_hz`.
/// The period is rounded to the nearest tick, the high phase is floored.
/// The 38 kHz / 33 % consumer-IR carrier is `carrier_ticks(80_000_000, 38_000, 33)`,
/// giving `Some((694, 1411))`.
///
/// Returns `None` if an argument is zero, `duty_percent` is 100 or more, or a
/// phase falls outside `1..=65535` ticks.
///
/// ```
/// use tamer::ir::carrier_ticks;
///
/// assert_eq!(carrier_ticks(80_000_000, 38_000, 33), Some((694, 1411)));
/// assert_eq!(carrier_ticks(80_000_000, 100, 50), None); // too slow for 16 bits
/// ```
#[must_use]
pub const fn carrier_ticks(clock_hz: u32, freq_hz: u32, duty_percent: u8) -> Option<(u16, u16)> {
    if clock_hz == 0 || freq_hz == 0 || duty_percent == 0 || duty_percent >= 100 {
        return None;
    }
    let total = (clock_hz as u64 + (freq_hz as u64) / 2) / freq_hz as u64;
    let high = total * duty_percent as u64 / 100;
    let low = total - high;
    if high == 0 || low == 0 || high > u16::MAX as u64 || low > u16::MAX as u64 {
        return None;
    }
    Some((high as u16, low as u16))
}

// ---------------------------------------------------------------------------
// Frame assembly (sans-io)
// ---------------------------------------------------------------------------

/// Turns edge timestamps into a pulse list.
///
/// Caller-driven in the `update(now)` style of
/// [`ToneSequencer`](crate::ToneSequencer): feed every line transition to
/// [`edge`](Self::edge), and poll [`update`](Self::update) at least once per
/// end-of-frame gap.
/// The caller resolves polarity: for an active-low receiver, `mark` is
/// "the pin went low".
/// Timestamps are `u64` microseconds from any monotonic clock.
///
/// A completed pulse shorter than `min_pulse_us` is a glitch: it is removed
/// and the neighbouring pulses are merged (the previous pulse is reopened), so
/// alternation and total duration are preserved.
/// There is no internal locking; if `edge` runs in an interrupt, the caller
/// serialises access (for example a critical-section mutex).
///
/// ```
/// use tamer::ir::{FrameAssembler, PulseCodec};
///
/// let mut asm = FrameAssembler::<80>::new(20, 150_000);
/// let codec = PulseCodec::nec();
/// let pulses = codec.encode::<67>(0x00FF_00FF).unwrap();
///
/// let mut t = 1_000u64;
/// let mut mark = true;
/// for &p in pulses.as_slice() {
///     asm.edge(t, mark);
///     t += u64::from(p);
///     mark = !mark;
/// }
/// asm.edge(t, false); // the trailer mark ends
/// assert!(asm.update(t + 1_000).is_none());
/// let frame = asm.update(t + 150_000).unwrap().unwrap();
/// assert_eq!(codec.decode(frame), Ok(0x00FF_00FF));
/// ```
#[derive(Clone, Debug)]
pub struct FrameAssembler<const N: usize> {
    min_pulse_us: u32,
    end_gap_us: u32,
    buf: [u32; N],
    len: usize,
    /// Level of the open (in-progress) interval.
    in_mark: bool,
    level_start_us: u64,
    last_edge_us: u64,
    active: bool,
    pending_reset: bool,
    overflow: bool,
}

impl<const N: usize> FrameAssembler<N> {
    /// Create an assembler holding up to `N` pulses.
    ///
    /// `min_pulse_us` is the glitch threshold and `end_gap_us` the idle time
    /// that ends a frame; the MicroPython reference uses 20 and 150 000.
    #[must_use]
    pub const fn new(min_pulse_us: u32, end_gap_us: u32) -> Self {
        Self {
            min_pulse_us,
            end_gap_us,
            buf: [0; N],
            len: 0,
            in_mark: false,
            level_start_us: 0,
            last_edge_us: 0,
            active: false,
            pending_reset: false,
            overflow: false,
        }
    }

    /// Late-arm: discard any frame and, if the line is already in a mark at
    /// `now_us`, open a frame whose first mark is measured from `now_us`
    /// (a truncated first mark).
    pub fn arm(&mut self, now_us: u64, in_mark: bool) {
        self.reset();
        if in_mark {
            self.begin(now_us);
        }
    }

    /// A line transition at `now_us`; `mark` is the state the line enters.
    ///
    /// While idle, a space edge is ignored and a mark edge opens a frame.
    /// A repeated edge to the current level is ignored.
    /// Non-monotonic timestamps clamp to zero-length pulses (then dropped by the
    /// glitch filter when `min_pulse_us > 0`; with 0 they are kept as zero-length pulses).
    pub fn edge(&mut self, now_us: u64, mark: bool) {
        if self.pending_reset {
            self.reset();
        }
        if !self.active {
            if mark {
                self.begin(now_us);
            }
            return;
        }
        if mark == self.in_mark {
            return;
        }
        let dur = now_us.saturating_sub(self.level_start_us);
        self.last_edge_us = self.last_edge_us.max(now_us);
        if dur < u64::from(self.min_pulse_us) && !self.overflow {
            // Glitch: drop it and reopen the previous pulse.
            match self.len.checked_sub(1) {
                Some(prev) => {
                    let p = self.buf.get(prev).copied().unwrap_or(0);
                    self.len = prev;
                    self.level_start_us = self.level_start_us.saturating_sub(u64::from(p));
                    self.in_mark = !self.in_mark;
                }
                // A glitch at the frame start: back to idle.
                None => self.active = false,
            }
            return;
        }
        let dur = u32::try_from(dur).unwrap_or(u32::MAX);
        match self.buf.get_mut(self.len) {
            Some(slot) if !self.overflow => {
                *slot = dur;
                self.len += 1;
            }
            _ => self.overflow = true,
        }
        self.in_mark = !self.in_mark;
        self.level_start_us = now_us;
    }

    /// Poll for a completed frame.
    ///
    /// A frame is complete once `now_us - last_edge >= end_gap_us`.
    /// The list holds completed pulses only: the in-progress interval (the
    /// gap) is dropped, so a frame ends on its last mark.
    /// An empty frame is discarded silently.
    /// The slice borrows the assembler; the state resets lazily on the next
    /// `edge`, `update`, or `arm`, so there is no release step.
    /// `update` must run before the next frame's first edge arrives, or the
    /// two merge.
    ///
    /// # Errors
    ///
    /// `Some(Err(`[`FrameError::Overflow`]`))` if more than `N` pulses arrived.
    #[must_use]
    pub fn update(&mut self, now_us: u64) -> Option<Result<&[u32], FrameError>> {
        if self.pending_reset {
            self.reset();
        }
        if !self.active || now_us.saturating_sub(self.last_edge_us) < u64::from(self.end_gap_us) {
            return None;
        }
        if self.len == 0 && !self.overflow {
            self.reset();
            return None;
        }
        self.pending_reset = true;
        if self.overflow {
            Some(Err(FrameError::Overflow))
        } else {
            Some(Ok(&self.buf[..self.len]))
        }
    }

    /// Assemble one already-delimited capture from `(mark, duration_us)`
    /// segments, such as RMT entries.
    ///
    /// Discards any frame in progress, then treats the segments as back-to-back
    /// levels starting at time 0.
    /// A leading space is ignored, consecutive segments of the same level
    /// merge, a zero duration ends the capture, and a final mark is closed at
    /// the end (a trailing space is left open and dropped).
    /// The end gap is waived: the capture is complete when the iterator ends.
    /// Returns what [`update`](Self::update) would, so `None` means no pulse
    /// survived the glitch filter.
    ///
    /// # Errors
    ///
    /// `Some(Err(`[`FrameError::Overflow`]`))` if more than `N` pulses arrived.
    ///
    /// ```
    /// use tamer::ir::FrameAssembler;
    ///
    /// let mut asm = FrameAssembler::<8>::new(20, 0);
    /// // leading space, mark 560, space 560, mark 560, then the idle tail (0)
    /// let segs = [(false, 100), (true, 560), (false, 560), (true, 560), (false, 0)];
    /// assert_eq!(asm.feed_segments(segs), Some(Ok(&[560, 560, 560][..])));
    /// ```
    #[must_use]
    pub fn feed_segments(
        &mut self,
        segments: impl IntoIterator<Item = (bool, u32)>,
    ) -> Option<Result<&[u32], FrameError>> {
        self.reset();
        let mut t: u64 = 0;
        let mut last_mark = false;
        for (mark, len) in segments {
            if len == 0 {
                break;
            }
            self.edge(t, mark);
            t += u64::from(len);
            last_mark = mark;
        }
        if last_mark {
            self.edge(t, false);
        }
        self.update(t.saturating_add(u64::from(self.end_gap_us)))
    }

    /// Discard any frame in progress and return to idle.
    pub fn reset(&mut self) {
        self.active = false;
        self.pending_reset = false;
        self.overflow = false;
        self.len = 0;
    }

    /// `true` while a frame is open.
    #[must_use]
    pub const fn is_receiving(&self) -> bool {
        self.active && !self.pending_reset
    }

    fn begin(&mut self, now_us: u64) {
        self.active = true;
        self.in_mark = true;
        self.level_start_us = now_us;
        self.last_edge_us = now_us;
        self.len = 0;
        self.overflow = false;
    }
}

// ---------------------------------------------------------------------------
// Bridge traits and mocks
// ---------------------------------------------------------------------------

/// Something that transmits a pulse list (microseconds, mark first).
///
/// Implemented by the hardware tier's transmitter; firmware that is generic
/// over it can be host-tested with [`MockPulseSink`].
pub trait PulseSink {
    /// Transmit error.
    type Error: fmt::Debug;

    /// Send one pulse list and return once it is out.
    ///
    /// # Errors
    ///
    /// Implementation-defined; see [`Self::Error`].
    fn send(&mut self, pulses: &[u32]) -> Result<(), Self::Error>;
}

/// Something that captures one pulse list (microseconds, mark first).
///
/// Implemented by the hardware tier's receiver; firmware that is generic over
/// it can be host-tested with [`MockPulseSource`].
pub trait PulseSource {
    /// Receive error.
    type Error: fmt::Debug;

    /// Wait up to `timeout_ms` for one frame.
    ///
    /// Returns `Ok(None)` on timeout; the slice borrows the source until the
    /// next call.
    ///
    /// # Errors
    ///
    /// Implementation-defined; see [`Self::Error`].
    fn receive(&mut self, timeout_ms: u32) -> Result<Option<&[u32]>, Self::Error>;
}

/// Recording [`PulseSink`] for host tests; keeps the last list sent.
///
/// `N` is the capacity of the recorded list; a longer one is truncated and
/// [`truncated`](Self::truncated) is set.
///
/// ```
/// use tamer::ir::{MockPulseSink, PulseSink};
///
/// let mut sink = MockPulseSink::<8>::new();
/// sink.send(&[9000, 4500, 560]).unwrap();
/// assert_eq!(sink.last(), &[9000, 4500, 560]);
/// assert_eq!(sink.send_count(), 1);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct MockPulseSink<const N: usize> {
    last: PulseBuf<N>,
    count: u32,
    truncated: bool,
}

impl<const N: usize> MockPulseSink<N> {
    /// Create a sink that has sent nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            last: PulseBuf::new(),
            count: 0,
            truncated: false,
        }
    }

    /// The most recent list sent (empty before the first send).
    #[must_use]
    pub fn last(&self) -> &[u32] {
        self.last.as_slice()
    }

    /// Number of `send` calls so far.
    #[must_use]
    pub const fn send_count(&self) -> u32 {
        self.count
    }

    /// `true` if the last list exceeded the capacity `N`.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

impl<const N: usize> Default for MockPulseSink<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> PulseSink for MockPulseSink<N> {
    type Error = core::convert::Infallible;

    fn send(&mut self, pulses: &[u32]) -> Result<(), Self::Error> {
        self.last.clear();
        self.truncated = false;
        for &p in pulses {
            if self.last.push(p).is_err() {
                self.truncated = true;
                break;
            }
        }
        self.count = self.count.saturating_add(1);
        Ok(())
    }
}

/// Scripted [`PulseSource`] for host tests; replays frames in order, then times out.
///
/// ```
/// use tamer::ir::{MockPulseSource, PulseSource};
///
/// let frames: [&[u32]; 1] = [&[9000, 2250, 560]];
/// let mut src = MockPulseSource::new(&frames);
/// assert_eq!(src.receive(100), Ok(Some(&[9000, 2250, 560][..])));
/// assert_eq!(src.receive(100), Ok(None));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct MockPulseSource<'a> {
    frames: &'a [&'a [u32]],
    next: usize,
}

impl<'a> MockPulseSource<'a> {
    /// Create a source that replays `frames`.
    #[must_use]
    pub const fn new(frames: &'a [&'a [u32]]) -> Self {
        Self { frames, next: 0 }
    }

    /// Frames not yet received.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.frames.len() - self.next
    }
}

impl PulseSource for MockPulseSource<'_> {
    type Error = core::convert::Infallible;

    fn receive(&mut self, _timeout_ms: u32) -> Result<Option<&[u32]>, Self::Error> {
        let frame = self.frames.get(self.next).copied();
        if frame.is_some() {
            self.next += 1;
        }
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aldi() -> PulseCodec {
        PulseCodec::new(
            Some(MarkSpace::new(8400, 4470)),
            MarkSpace::new(560, 540),
            MarkSpace::new(1000, 1000),
            Some(560),
            8,
            BitOrder::LsbFirst,
            Tolerance::Percent(25),
        )
        .unwrap()
    }

    /// Real captures: "level duration" pairs; level 0 is a mark. The final
    /// pair is the idle gap and is stripped.
    const CAPTURES: [(&str, u32); 6] = [
        ("0 6339 1 4472 0 564 1 536 0 563 1 547 0 572 1 536 0 564 1 541 0 564 1 536 0 583 1 545 0 996 1 994 0 1015 1 1013 0 545 1 150018", 0xC0),
        ("0 8721 1 4462 0 579 1 536 0 566 1 536 0 578 1 546 0 562 1 521 0 1044 1 972 0 558 1 538 0 1029 1 972 0 578 1 532 0 578 1 150015", 0x50),
        ("0 2465, 1 4447, 0 579, 1 524, 0 578, 1 544, 0 566, 1 537, 0 578, 1 531, 0 1014, 1 993, 0 566, 1 537, 0 1021, 1 1014, 0 981, 1 979, 0 579, 1 150017", 0xD0),
        ("0 61, 1 4449, 0 563, 1 536, 0 588, 1 530, 0 558, 1 536, 0 588, 1 521, 0 1035, 1 981, 0 558, 1 536, 0 588, 1 536, 0 1014, 1 997, 0 557, 1 150032", 0x90),
        ("0 6765, 1 4452, 0 559, 1 536, 0 566, 1 548, 0 563, 1 546, 0 558, 1 542, 0 1023, 1 993, 0 1014, 1 973, 0 1023, 1 993, 0 557, 1 532, 0 580, 1 150019", 0x70),
        ("0 68, 1 4448, 0 578, 1 537, 0 566, 1 540, 0 557, 1 537, 0 587, 1 523, 0 1021, 1 981, 0 1015, 1 993, 0 566, 1 536, 0 579, 1 532, 0 578, 1 150020", 0x30),
    ];

    fn capture(raw: &str) -> std::vec::Vec<u32> {
        let nums: std::vec::Vec<u32> = raw
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().unwrap())
            .collect();
        let mut durs: std::vec::Vec<u32> = nums.chunks(2).map(|c| c[1]).collect();
        durs.pop(); // the idle gap
        durs
    }

    /// Feed pulses as edges starting at `t0`; returns the time of the final edge.
    fn feed<const N: usize>(asm: &mut FrameAssembler<N>, pulses: &[u32], t0: u64) -> u64 {
        let mut t = t0;
        let mut mark = true;
        for &p in pulses {
            asm.edge(t, mark);
            t += u64::from(p);
            mark = !mark;
        }
        asm.edge(t, mark);
        t
    }

    #[test]
    fn pulse_buf_push_to_capacity_then_error() {
        let mut b = PulseBuf::<2>::new();
        assert!(b.is_empty());
        assert_eq!(b.capacity(), 2);
        b.push(1).unwrap();
        b.push(2).unwrap();
        assert_eq!(b.push(3), Err(EncodeError::BufferTooSmall { needed: 3 }));
        assert_eq!(b.as_slice(), &[1, 2]);
        b.clear();
        assert_eq!(b.len(), 0);
    }

    #[test]
    fn pulse_pairs_even_odd_empty() {
        let v: std::vec::Vec<_> = pulse_pairs(&[1, 2, 3, 4]).collect();
        assert_eq!(v, [(1, 2), (3, 4)]);
        let v: std::vec::Vec<_> = pulse_pairs(&[1, 2, 3]).collect();
        assert_eq!(v, [(1, 2), (3, 0)]);
        assert_eq!(pulse_pairs(&[]).count(), 0);
    }

    fn codec(bits: u8, order: BitOrder, trailer: Option<u32>) -> Result<PulseCodec, CodecError> {
        PulseCodec::new(
            None,
            MarkSpace::new(500, 500),
            MarkSpace::new(500, 1500),
            trailer,
            bits,
            order,
            Tolerance::Percent(20),
        )
    }

    #[test]
    fn codec_new_rejects_bits_0_and_33() {
        assert_eq!(
            codec(0, BitOrder::LsbFirst, None),
            Err(CodecError::InvalidBitCount)
        );
        assert_eq!(
            codec(33, BitOrder::LsbFirst, None),
            Err(CodecError::InvalidBitCount)
        );
    }

    #[test]
    fn codec_new_accepts_bits_1_and_32() {
        assert!(codec(1, BitOrder::LsbFirst, None).is_ok());
        assert!(codec(32, BitOrder::LsbFirst, None).is_ok());
    }

    #[test]
    fn codec_new_rejects_overlapping_bit_windows() {
        let r = PulseCodec::new(
            None,
            MarkSpace::new(500, 500),
            MarkSpace::new(550, 600),
            None,
            8,
            BitOrder::LsbFirst,
            Tolerance::Percent(25),
        );
        assert_eq!(r, Err(CodecError::OverlappingBits));
        // Differing in only one field is fine.
        assert!(codec(8, BitOrder::LsbFirst, None).is_ok());
    }

    #[test]
    fn nec_pulse_len_is_67() {
        assert_eq!(PulseCodec::nec().pulse_len(), 67);
        assert_eq!(PulseCodec::nec().bits(), 32);
        assert_eq!(PulseCodec::nec().bit_order(), BitOrder::LsbFirst);
    }

    #[test]
    fn aldi_pulse_len_is_19() {
        assert_eq!(aldi().pulse_len(), 19);
    }

    #[test]
    fn encode_value_too_wide() {
        let mut out = [0u32; 32];
        assert_eq!(
            aldi().encode_into(0x100, &mut out),
            Err(EncodeError::ValueTooWide)
        );
    }

    #[test]
    fn encode_buffer_too_small_reports_needed() {
        let mut out = [0u32; 18];
        assert_eq!(
            aldi().encode_into(1, &mut out),
            Err(EncodeError::BufferTooSmall { needed: 19 })
        );
        assert_eq!(
            aldi().encode::<10>(1),
            Err(EncodeError::BufferTooSmall { needed: 19 })
        );
    }

    #[test]
    fn encode_msb_vs_lsb_first() {
        let mut out = [0u32; 8];
        let lsb = codec(4, BitOrder::LsbFirst, None).unwrap();
        let n = lsb.encode_into(0b0001, &mut out).unwrap();
        assert_eq!(n, 8);
        assert_eq!(out, [500, 1500, 500, 500, 500, 500, 500, 500]);
        let msb = codec(4, BitOrder::MsbFirst, None).unwrap();
        msb.encode_into(0b0001, &mut out).unwrap();
        assert_eq!(out, [500, 500, 500, 500, 500, 500, 500, 1500]);
    }

    #[test]
    fn nec_round_trip_samples_incl_00_and_ff() {
        let c = PulseCodec::nec();
        for &a in &[0x00u8, 0x01, 0x20, 0x7F, 0x80, 0xFF] {
            for &cmd in &[0x00u8, 0x0B, 0x55, 0xAA, 0xFF] {
                let f = NecFrame::new(a, cmd);
                let p = c.encode::<67>(f.to_word()).unwrap();
                assert_eq!(p.len(), 67);
                assert_eq!(NecFrame::from_word(c.decode(p.as_slice()).unwrap()), Ok(f));
            }
        }
    }

    #[test]
    fn aldi_8bit_round_trip_all_256() {
        let c = aldi();
        for v in 0..=255u32 {
            let p = c.encode::<19>(v).unwrap();
            assert_eq!(c.decode(p.as_slice()), Ok(v));
        }
    }

    #[test]
    fn one_bit_and_32_bit_msb_round_trip() {
        let one = codec(1, BitOrder::LsbFirst, Some(500)).unwrap();
        for v in 0..2 {
            let p = one.encode::<8>(v).unwrap();
            assert_eq!(one.decode(p.as_slice()), Ok(v));
        }
        let c = codec(32, BitOrder::MsbFirst, Some(500)).unwrap();
        for v in [0, 0xFFFF_FFFF, 0x8000_0001, 0x1234_5678] {
            let p = c.encode::<67>(v).unwrap();
            assert_eq!(c.decode(p.as_slice()), Ok(v));
        }
    }

    #[test]
    fn tolerance_boundaries_inclusive_25pct() {
        let t = Tolerance::Percent(25);
        assert!(t.matches(560, 700));
        assert!(!t.matches(560, 701));
        assert!(t.matches(560, 420));
        assert!(!t.matches(560, 419));
        assert!(t.at_most(9000, 11250));
        assert!(!t.at_most(9000, 11251));
        assert!(t.at_most(9000, 0));
    }

    #[test]
    fn tolerance_micros_variant() {
        let t = Tolerance::Micros(100);
        assert!(t.matches(1000, 1100));
        assert!(!t.matches(1000, 1101));
        assert!(t.matches(1000, 900));
        assert!(!t.matches(1000, 899));
        assert!(t.at_most(1000, 1100));
        assert!(!t.at_most(1000, 1101));
    }

    #[test]
    fn tolerance_percent_zero_exact_only() {
        let t = Tolerance::Percent(0);
        assert!(t.matches(560, 560));
        assert!(!t.matches(560, 561));
        assert!(!t.matches(560, 559));
    }

    #[test]
    fn tolerance_zero_nominal_no_div_by_zero() {
        let t = Tolerance::Percent(25);
        assert!(t.matches(0, 0));
        assert!(!t.matches(0, 1));
        assert!(t.at_most(0, 0));
        assert!(Tolerance::Percent(250).matches(100, 200));
        assert!(!Tolerance::Percent(250).matches(100, 201));
        assert!(Tolerance::Micros(u32::MAX).matches(0, u32::MAX));
    }

    fn nec_with_header(mark: u32, space: u32) -> std::vec::Vec<u32> {
        let mut p = std::vec::Vec::from(PulseCodec::nec().encode::<67>(0).unwrap().as_slice());
        p[0] = mark;
        p[1] = space;
        p
    }

    #[test]
    fn truncated_header_mark_0_61_8721_accepted() {
        let c = PulseCodec::nec();
        for m in [0, 61, 8721, 11250] {
            assert_eq!(c.decode(&nec_with_header(m, 4500)), Ok(0), "mark {m}");
        }
    }

    #[test]
    fn header_mark_over_upper_bound_rejected() {
        assert_eq!(
            PulseCodec::nec().decode(&nec_with_header(11251, 4500)),
            Err(DecodeError::HeaderMismatch)
        );
    }

    #[test]
    fn header_space_wrong_rejected() {
        let c = PulseCodec::nec();
        assert_eq!(
            c.decode(&nec_with_header(9000, 2250)),
            Err(DecodeError::HeaderMismatch)
        );
        assert_eq!(c.decode(&[9000]), Err(DecodeError::HeaderMismatch));
        assert_eq!(c.decode(&[]), Err(DecodeError::Empty));
    }

    #[test]
    fn wrong_bit_count_24_33_31() {
        let c = PulseCodec::nec();
        let c24 = PulseCodec::new(
            Some(MarkSpace::new(9000, 4500)),
            MarkSpace::new(560, 560),
            MarkSpace::new(560, 1690),
            Some(560),
            24,
            BitOrder::LsbFirst,
            Tolerance::Percent(25),
        )
        .unwrap();
        let p24 = c24.encode::<67>(0x123456).unwrap();
        assert_eq!(
            c.decode(p24.as_slice()),
            Err(DecodeError::WrongBitCount {
                expected: 32,
                found: 24
            })
        );
        let full = c.encode::<67>(1).unwrap();
        let mut p33 = std::vec::Vec::from(full.as_slice());
        p33.splice(66..66, [560, 560]);
        assert_eq!(
            c.decode(&p33),
            Err(DecodeError::WrongBitCount {
                expected: 32,
                found: 33
            })
        );
        let mut p31 = std::vec::Vec::from(full.as_slice());
        p31.drain(64..66);
        assert_eq!(
            c.decode(&p31),
            Err(DecodeError::WrongBitCount {
                expected: 32,
                found: 31
            })
        );
    }

    #[test]
    fn missing_or_extra_trailer_pulses() {
        let c = PulseCodec::nec();
        let p = c.encode::<67>(0xDEAD_BEEF).unwrap();
        assert!(matches!(
            c.decode(&p.as_slice()[..66]),
            Err(DecodeError::WrongBitCount { .. })
        ));
        let mut extra = std::vec::Vec::from(p.as_slice());
        extra.push(560);
        assert!(matches!(
            c.decode(&extra),
            Err(DecodeError::WrongBitCount { .. })
        ));
        let mut bad = std::vec::Vec::from(p.as_slice());
        bad[66] = 2000;
        assert_eq!(c.decode(&bad), Err(DecodeError::TrailerMismatch));
        bad[66] = 560;
        bad[10] = 5000;
        assert_eq!(c.decode(&bad), Err(DecodeError::BitMismatch { index: 4 }));
    }

    #[test]
    fn trailerless_codec_last_space_omitted() {
        let c = PulseCodec::new(
            None,
            MarkSpace::new(500, 500),
            MarkSpace::new(1000, 500),
            None,
            4,
            BitOrder::LsbFirst,
            Tolerance::Percent(20),
        )
        .unwrap();
        let p = c.encode::<8>(0b1010).unwrap();
        assert_eq!(c.decode(p.as_slice()), Ok(0b1010));
        assert_eq!(c.decode(&p.as_slice()[..7]), Ok(0b1010));
        assert_eq!(
            c.decode(&p.as_slice()[..6]),
            Err(DecodeError::WrongBitCount {
                expected: 4,
                found: 3
            })
        );
        // Identical marks make a lone last mark ambiguous.
        let amb = codec(4, BitOrder::LsbFirst, None).unwrap();
        assert_eq!(
            amb.decode(&[500, 500, 500, 500, 500, 500, 500]),
            Err(DecodeError::BitMismatch { index: 3 })
        );
    }

    #[test]
    fn golden_captures_decode_via_aldi_codec_six_buttons() {
        for (raw, want) in CAPTURES {
            assert_eq!(aldi().decode(&capture(raw)), Ok(want), "{raw}");
        }
    }

    #[test]
    fn golden_captures_fail_nec_with_wrong_bit_count() {
        for (raw, _) in CAPTURES {
            assert_eq!(
                PulseCodec::nec().decode(&capture(raw)),
                Err(DecodeError::WrongBitCount {
                    expected: 32,
                    found: 8
                })
            );
        }
    }

    #[test]
    fn nec_frame_word_round_trip() {
        let f = NecFrame::new(0x12, 0x34);
        assert_eq!(f.to_word(), 0xCB34_ED12);
        assert_eq!(NecFrame::from_word(f.to_word()), Ok(f));
    }

    #[test]
    fn nec_inverted_byte_mismatch_address_and_command() {
        let w = NecFrame::new(0x12, 0x34).to_word();
        assert_eq!(
            NecFrame::from_word(w ^ 0x0000_0100),
            Err(NecError::InvertedByteMismatch)
        );
        assert_eq!(
            NecFrame::from_word(w ^ 0x0100_0000),
            Err(NecError::InvertedByteMismatch)
        );
    }

    #[test]
    fn nec_extended_address() {
        let w = 0xCB34_A512;
        assert_eq!(NecFrame::from_word_extended(w), Ok((0xA512, 0x34)));
        assert_eq!(NecFrame::from_word(w), Err(NecError::InvertedByteMismatch));
        assert_eq!(
            NecFrame::from_word_extended(w ^ 0x0100_0000),
            Err(NecError::InvertedByteMismatch)
        );
    }

    #[test]
    fn nec_repeat_detected_and_data_frame_rejected() {
        assert!(is_nec_repeat(&NEC_REPEAT));
        assert!(!is_nec_repeat(
            PulseCodec::nec().encode::<67>(1).unwrap().as_slice()
        ));
        assert!(!is_nec_repeat(&[9000, 4500, 560]));
        assert!(!is_nec_repeat(&[]));
    }

    #[test]
    fn nec_repeat_with_truncated_first_mark() {
        assert!(is_nec_repeat(&[61, 2250, 560]));
        assert!(!is_nec_repeat(&[20000, 2250, 560]));
    }

    #[test]
    fn assembler_golden_captures_end_to_end() {
        let mut asm = FrameAssembler::<80>::new(20, 150_000);
        let mut t = 5_000u64;
        for (raw, want) in CAPTURES {
            let pulses = capture(raw);
            let end = feed(&mut asm, &pulses, t);
            assert!(asm.is_receiving());
            let frame = asm.update(end + 150_000).unwrap().unwrap();
            assert_eq!(frame, pulses.as_slice());
            assert_eq!(aldi().decode(frame), Ok(want));
            t = end + 300_000;
        }
    }

    #[test]
    fn assembler_glitch_in_mark_merged() {
        let mut asm = FrameAssembler::<8>::new(20, 1_000);
        asm.edge(0, true);
        asm.edge(300, false);
        asm.edge(305, true); // 5 us space glitch
        asm.edge(600, false);
        asm.edge(1_000, true);
        asm.edge(1_500, false);
        let f = asm.update(10_000).unwrap().unwrap();
        assert_eq!(f, &[600, 400, 500]);
    }

    #[test]
    fn assembler_glitch_at_start_dropped() {
        let mut asm = FrameAssembler::<8>::new(20, 1_000);
        asm.edge(0, true);
        asm.edge(5, false); // 5 us mark
        assert!(!asm.is_receiving());
        asm.edge(500, true);
        asm.edge(900, false);
        assert_eq!(asm.update(5_000).unwrap().unwrap(), &[400]);
    }

    #[test]
    fn assembler_idle_space_edge_ignored() {
        let mut asm = FrameAssembler::<8>::new(20, 1_000);
        asm.edge(100, false);
        assert!(!asm.is_receiving());
        assert!(asm.update(1_000_000).is_none());
    }

    #[test]
    fn assembler_arm_in_mark_truncated_first_mark_decodes() {
        let c = PulseCodec::nec();
        let full = c.encode::<67>(0x00FF_00FF).unwrap();
        let mut asm = FrameAssembler::<80>::new(20, 150_000);
        asm.arm(1_000, true);
        assert!(asm.is_receiving());
        let mut p = std::vec::Vec::from(full.as_slice());
        p[0] = 3_000; // truncated
        let end = feed_from_second(&mut asm, &p, 1_000);
        let frame = asm.update(end + 150_000).unwrap().unwrap();
        assert_eq!(frame[0], 3_000);
        assert_eq!(c.decode(frame), Ok(0x00FF_00FF));
        asm.arm(0, false);
        assert!(!asm.is_receiving());
    }

    /// Like `feed`, but the first mark is already open (armed).
    fn feed_from_second<const N: usize>(
        asm: &mut FrameAssembler<N>,
        pulses: &[u32],
        t0: u64,
    ) -> u64 {
        let mut t = t0 + u64::from(pulses[0]);
        let mut mark = false;
        asm.edge(t, mark);
        for &p in &pulses[1..] {
            t += u64::from(p);
            mark = !mark;
            asm.edge(t, mark);
        }
        t
    }

    #[test]
    fn assembler_gap_boundary_149999_vs_150000() {
        let mut asm = FrameAssembler::<8>::new(20, 150_000);
        asm.edge(0, true);
        asm.edge(500, false);
        assert!(asm.update(500 + 149_999).is_none());
        assert!(asm.update(500 + 150_000).is_some());
    }

    #[test]
    fn assembler_overflow_reports_error_drops_excess() {
        let mut asm = FrameAssembler::<2>::new(20, 1_000);
        feed(&mut asm, &[100, 100, 100, 100, 100], 0);
        assert_eq!(asm.update(100_000), Some(Err(FrameError::Overflow)));
        // Lazily reset afterwards.
        assert!(asm.update(200_000).is_none());
        assert!(!asm.is_receiving());
    }

    #[test]
    fn assembler_back_to_back_frames_lazy_reset() {
        let mut asm = FrameAssembler::<8>::new(20, 1_000);
        let e = feed(&mut asm, &[100, 200, 300], 0);
        assert_eq!(asm.update(e + 1_000).unwrap().unwrap(), &[100, 200, 300]);
        assert!(!asm.is_receiving());
        let e = feed(&mut asm, &[400, 500, 600], e + 5_000);
        assert_eq!(asm.update(e + 1_000).unwrap().unwrap(), &[400, 500, 600]);
        assert!(asm.update(e + 1_000).is_none());
    }

    #[test]
    fn assembler_non_monotonic_timestamps_no_panic() {
        let mut asm = FrameAssembler::<8>::new(20, 1_000);
        asm.edge(1_000, true);
        asm.edge(500, false); // goes backwards: a zero-length pulse
        asm.edge(0, true);
        asm.edge(u64::MAX, false);
        let _ = asm.update(0);
        let _ = asm.update(u64::MAX);
        asm.arm(u64::MAX, true);
        asm.edge(0, false);
    }

    #[test]
    fn assembler_empty_frame_none() {
        let mut asm = FrameAssembler::<8>::new(20, 1_000);
        asm.edge(0, true); // only a first mark, never completed
        assert!(asm.update(100_000).is_none());
        assert!(!asm.is_receiving());
    }

    #[test]
    fn errors_display() {
        use std::string::ToString;
        assert_eq!(
            EncodeError::BufferTooSmall { needed: 3 }.to_string(),
            "output buffer too small, 3 pulses needed"
        );
        assert_eq!(
            DecodeError::WrongBitCount {
                expected: 32,
                found: 8
            }
            .to_string(),
            "expected 32 bits, found 8"
        );
        assert!(!CodecError::InvalidBitCount.to_string().is_empty());
        assert!(!NecError::InvertedByteMismatch.to_string().is_empty());
        assert!(!FrameError::Overflow.to_string().is_empty());
    }

    #[test]
    fn carrier_ticks_nec_at_80_mhz() {
        assert_eq!(carrier_ticks(80_000_000, 38_000, 33), Some((694, 1411)));
        assert_eq!(carrier_ticks(80_000_000, 56_000, 50), Some((714, 715)));
    }

    #[test]
    fn carrier_ticks_rejects_out_of_range() {
        assert_eq!(carrier_ticks(0, 38_000, 33), None);
        assert_eq!(carrier_ticks(80_000_000, 0, 33), None);
        assert_eq!(carrier_ticks(80_000_000, 38_000, 0), None);
        assert_eq!(carrier_ticks(80_000_000, 38_000, 100), None);
        // 1 MHz carrier in an 1 MHz clock: one tick cannot hold two phases.
        assert_eq!(carrier_ticks(1_000_000, 1_000_000, 50), None);
        // Phase over 65535 ticks.
        assert_eq!(carrier_ticks(80_000_000, 100, 50), None);
    }

    #[test]
    fn assembler_min_pulse_boundary_19_dropped_20_kept() {
        let mut asm = FrameAssembler::<8>::new(20, 1_000);
        asm.edge(0, true);
        asm.edge(500, false);
        asm.edge(519, true); // 19 us space: dropped
        asm.edge(1_000, false);
        assert_eq!(asm.update(10_000).unwrap().unwrap(), &[1_000]);

        let mut asm = FrameAssembler::<8>::new(20, 1_000);
        asm.edge(0, true);
        asm.edge(500, false);
        asm.edge(520, true); // 20 us space: kept
        asm.edge(1_000, false);
        assert_eq!(asm.update(10_000).unwrap().unwrap(), &[500, 20, 480]);
    }

    #[test]
    fn assembler_min_pulse_zero_keeps_everything() {
        let mut asm = FrameAssembler::<8>::new(0, 1_000);
        asm.edge(0, true);
        asm.edge(1, false);
        asm.edge(1, true); // zero-length space
        asm.edge(3, false);
        assert_eq!(asm.update(10_000).unwrap().unwrap(), &[1, 0, 2]);
    }

    #[test]
    fn assembler_glitch_after_overflow_still_overflow() {
        let mut asm = FrameAssembler::<2>::new(20, 1_000);
        asm.edge(0, true);
        asm.edge(100, false);
        asm.edge(200, true);
        asm.edge(300, false); // third pulse: overflow
        asm.edge(305, true); // glitch after overflow is stored, not merged
        assert_eq!(asm.update(10_000), Some(Err(FrameError::Overflow)));
    }

    #[test]
    fn feed_segments_basic_and_end_marker() {
        let mut asm = FrameAssembler::<8>::new(20, 0);
        let segs = [(true, 500), (false, 400), (true, 300), (false, 0)];
        assert_eq!(asm.feed_segments(segs).unwrap().unwrap(), &[500, 400, 300]);
    }

    #[test]
    fn feed_segments_leading_space_ignored() {
        let mut asm = FrameAssembler::<8>::new(20, 0);
        let segs = [(false, 700), (true, 500), (false, 400), (true, 300)];
        assert_eq!(asm.feed_segments(segs).unwrap().unwrap(), &[500, 400, 300]);
    }

    #[test]
    fn feed_segments_same_level_merge() {
        let mut asm = FrameAssembler::<8>::new(20, 0);
        let segs = [
            (true, 200),
            (true, 300),
            (false, 400),
            (false, 100),
            (true, 50),
        ];
        assert_eq!(asm.feed_segments(segs).unwrap().unwrap(), &[500, 500, 50]);
    }

    #[test]
    fn feed_segments_trailing_space_dropped() {
        let mut asm = FrameAssembler::<8>::new(20, 0);
        let segs = [(true, 500), (false, 400), (true, 300), (false, 9_000)];
        assert_eq!(asm.feed_segments(segs).unwrap().unwrap(), &[500, 400, 300]);
    }

    #[test]
    fn feed_segments_trailing_mark_closed() {
        let mut asm = FrameAssembler::<8>::new(20, 0);
        assert_eq!(asm.feed_segments([(true, 560)]).unwrap().unwrap(), &[560]);
    }

    #[test]
    fn feed_segments_empty_and_space_only_none() {
        let mut asm = FrameAssembler::<8>::new(20, 0);
        assert!(asm.feed_segments([]).is_none());
        assert!(asm.feed_segments([(false, 500), (false, 0)]).is_none());
        assert!(asm.feed_segments([(true, 5)]).is_none()); // lone glitch
    }

    #[test]
    fn feed_segments_overflow_and_reuse() {
        let mut asm = FrameAssembler::<2>::new(20, 0);
        let segs = [(true, 100), (false, 100), (true, 100), (false, 100)];
        assert_eq!(asm.feed_segments(segs), Some(Err(FrameError::Overflow)));
        // The assembler is reusable afterwards.
        assert_eq!(
            asm.feed_segments([(true, 100), (false, 100)])
                .unwrap()
                .unwrap(),
            &[100]
        );
    }

    #[test]
    fn feed_segments_honours_nonzero_end_gap() {
        let mut asm = FrameAssembler::<8>::new(20, 150_000);
        assert_eq!(asm.feed_segments([(true, 500)]).unwrap().unwrap(), &[500]);
    }

    #[test]
    fn feed_segments_decodes_golden_capture() {
        let pulses = capture(CAPTURES[0].0);
        let mut asm = FrameAssembler::<80>::new(20, 0);
        let mut mark = true;
        let segs = pulses.iter().map(|&p| {
            let s = (mark, p);
            mark = !mark;
            s
        });
        let frame = asm.feed_segments(segs).unwrap().unwrap();
        assert_eq!(aldi().decode(frame), Ok(CAPTURES[0].1));
    }

    #[test]
    fn validate_pulses_cases() {
        assert_eq!(validate_pulses(&[], 100), Err(PulseListError::Empty));
        assert_eq!(
            validate_pulses(&[1, 0, 1], 100),
            Err(PulseListError::Zero { index: 1 })
        );
        assert_eq!(
            validate_pulses(&[1, 101], 100),
            Err(PulseListError::TooLong { index: 1 })
        );
        assert_eq!(validate_pulses(&[100, 1, 100], 100), Ok(()));
        assert_eq!(validate_pulses(&[1], 100), Ok(()));
    }

    #[test]
    fn mock_sink_records_and_truncates() {
        let mut sink = MockPulseSink::<2>::new();
        assert_eq!(sink.send_count(), 0);
        sink.send(&[1, 2]).unwrap();
        assert_eq!((sink.last(), sink.truncated()), (&[1, 2][..], false));
        sink.send(&[1, 2, 3]).unwrap();
        assert_eq!((sink.last(), sink.truncated()), (&[1, 2][..], true));
        assert_eq!(sink.send_count(), 2);
    }

    #[test]
    fn mock_source_replays_then_times_out() {
        let frames: [&[u32]; 2] = [&[1, 2], &[3]];
        let mut src = MockPulseSource::new(&frames);
        assert_eq!(src.remaining(), 2);
        assert_eq!(src.receive(10), Ok(Some(&[1, 2][..])));
        assert_eq!(src.receive(10), Ok(Some(&[3][..])));
        assert_eq!(src.receive(10), Ok(None));
        assert_eq!(src.remaining(), 0);
    }

    #[test]
    fn tolerance_window_is_conservative() {
        // 25 % of 10 is 2.5: window rounds up to 3, so validation never accepts
        // a pair that `matches` could still see as ambiguous.
        assert_eq!(Tolerance::Percent(25).window(10), (7, 13));
    }
}
