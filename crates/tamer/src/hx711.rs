//! Avia HX711 24-bit load-cell ADC: protocol constants, sign extension, and the tare / scale / temperature-compensation math.
//!
//! Provides platform-independent types and functions for the HX711 and the
//! weigh pipeline that turns its raw counts into grams.
//! The pure part imports no HAL or chip crate.
//! With the `hal` feature, `Hx711` adds a blocking driver over
//! `embedded_hal::digital::{InputPin, OutputPin}` and `DelayNs`
//! (see ADR-016), so the same code runs on esp-hal and esp-idf-hal.
//!
//! This module is device-specific by design: the pulse counts and timings are
//! fixed HX711 datasheet facts, not a generic ADC abstraction.
//! The NAU7802, calibration storage and a calibration downlink are out of scope.
//!
//! # Protocol
//!
//! The chip has two lines, DOUT (data out, an input for the host) and PD_SCK
//! (power-down and serial clock, an output for the host).
//! DOUT going low signals that a conversion is ready.
//! The host then clocks 24 data bits out MSB first, one PD_SCK pulse each,
//! followed by 1, 2 or 3 extra pulses ([`Gain::extra_pulses`](crate::hx711::Gain::extra_pulses)) that select the
//! channel and gain of the **next** conversion.
//! The 24-bit value is two's complement ([`sign_extend_24`](crate::hx711::sign_extend_24)); the full range is
//! [`RAW_MIN`](crate::hx711::RAW_MIN) to [`RAW_MAX`](crate::hx711::RAW_MAX).
//!
//! Holding PD_SCK high for [`POWER_DOWN_US`](crate::hx711::POWER_DOWN_US) (60 us) or longer powers the chip
//! down.
//! A PD_SCK pulse of that length inside a read would therefore power the chip
//! down mid-read, which is why the pulse train must run with interrupts masked
//! ([`InterruptMask`](crate::hx711::InterruptMask), ADR-016).
//! After power-up or a gain change the chip needs a settling time
//! ([`SETTLE_MS_10HZ`](crate::hx711::SETTLE_MS_10HZ), [`SETTLE_MS_80HZ`](crate::hx711::SETTLE_MS_80HZ), depending on the RATE pin) and the
//! first sample after it is to be discarded by the caller.
//!
//! # Timing
//!
//! Every PD_SCK phase, high and low, lasts [`PULSE_US`](crate::hx711::PULSE_US) microseconds.
//! The masked pulse train is 25 to 27 pulses, so with the GPIO overhead the
//! interrupt-masked window is about 100 to 135 us; the upper bound is an
//! estimate until a rig run measures it.
//! The hazard it is compared against is [`POWER_DOWN_US`](crate::hx711::POWER_DOWN_US): an interrupt that
//! held PD_SCK high that long would power the chip down mid-read.
//!
//! # Calibration byte layout
//!
//! [`Calibration::to_bytes`](crate::hx711::Calibration::to_bytes) writes 16 bytes, all little endian.
//!
//! | Bytes | Field |
//! |:--|:--|
//! | `[0]` | [`CALIBRATION_MAGIC`](crate::hx711::CALIBRATION_MAGIC) (`0xC1`) |
//! | `[1]` | [`CALIBRATION_VERSION`](crate::hx711::CALIBRATION_VERSION) (`1`) |
//! | `[2..6]` | tare offset, `i32` |
//! | `[6..10]` | counts per gram, `f32` |
//! | `[10..12]` | reference temperature in deci-degrees Celsius, `i16` |
//! | `[12..16]` | drift in grams per degree Celsius, `f32` |
//!
//! [`Calibration::from_bytes`](crate::hx711::Calibration::from_bytes) rejects a wrong magic or version and any
//! content that [`Calibration::new`](crate::hx711::Calibration::new) would reject.
//!
//! # Example
//!
//! ```
//! use tamer::hx711::{Calibration, LoadCellScale};
//!
//! let mut scale = LoadCellScale::<4>::new(Calibration::identity());
//!
//! // Empty platform: tare on an averaged raw reading.
//! scale.tare(8_000_000);
//!
//! // Known 10 kg on the platform reads 8 210 000 counts.
//! scale.calibrate(8_210_000, 10_000.0).unwrap();
//!
//! // 5 kg later.
//! let grams = scale.weigh(8_105_000, None);
//! assert!((grams - 5_000.0).abs() < 1.0);
//!
//! // Persist and restore the calibration.
//! let bytes = scale.calibration().to_bytes();
//! assert_eq!(Calibration::from_bytes(&bytes), Some(scale.calibration()));
//! ```

use crate::smoothing::SignedSlidingAverage;

/// Smallest raw value the 24-bit converter can report.
pub const RAW_MIN: i32 = -8_388_608;
/// Largest raw value the 24-bit converter can report.
pub const RAW_MAX: i32 = 8_388_607;
/// Datasheet threshold: PD_SCK held high this long (in microseconds) powers the chip down.
pub const POWER_DOWN_US: u32 = 60;
/// Settling time after power-up or a gain change at the 10 Hz output rate, in milliseconds.
pub const SETTLE_MS_10HZ: u32 = 400;
/// Settling time after power-up or a gain change at the 80 Hz output rate, in milliseconds.
pub const SETTLE_MS_80HZ: u32 = 50;
/// Suggested interval between DOUT polls while waiting for a conversion, in microseconds.
pub const POLL_INTERVAL_US: u32 = 100;
/// Length of each PD_SCK high and low phase during a read, in microseconds.
///
/// esp-hal 1.2.2 `Delay` spins on an `Instant` floored to whole microseconds
/// (`delay.rs:53-57`, `time.rs:715-719`), so a 1 us request lasts anywhere in (0, 1] us.
/// esp-idf-hal `Ets` waits the full time.
/// 2 us guarantees more than 1 us on both tiers.
/// That clears the HX711 PD_SCK high and low minimum of 0.2 us (T3/T4)
/// and the DOUT settling time of 0.1 us (T2).
/// It stays far below the 50 us maximum high time.
pub const PULSE_US: u32 = 2;
/// First byte of the serialized [`Calibration`].
pub const CALIBRATION_MAGIC: u8 = 0xC1;
/// Second byte of the serialized [`Calibration`]: the layout version.
pub const CALIBRATION_VERSION: u8 = 1;

/// Channel and gain selection, encoded as the number of extra PD_SCK pulses after the 24 data bits.
///
/// The selection applies to the **next** conversion, not to the one being read.
/// The sample after a change is therefore taken with the old setting or is still
/// settling, and the caller discards it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gain {
    /// Channel A, gain 128 (one extra pulse).
    A128,
    /// Channel B, gain 32 (two extra pulses).
    B32,
    /// Channel A, gain 64 (three extra pulses).
    A64,
}

impl Gain {
    /// Returns the number of PD_SCK pulses after the 24 data bits: 1, 2 or 3.
    #[must_use]
    pub const fn extra_pulses(self) -> u8 {
        match self {
            Self::A128 => 1,
            Self::B32 => 2,
            Self::A64 => 3,
        }
    }

    /// Returns the total number of PD_SCK pulses for one read: 25, 26 or 27.
    #[must_use]
    pub const fn total_pulses(self) -> u8 {
        24 + self.extra_pulses()
    }
}

/// Sign-extends the low 24 bits of `raw` to an `i32`.
///
/// Bits above bit 23 are ignored; bit 23 is the sign.
///
/// ```
/// use tamer::hx711::sign_extend_24;
///
/// assert_eq!(sign_extend_24(0x12_3456), 0x12_3456);
/// assert_eq!(sign_extend_24(0x80_0000), -8_388_608);
/// assert_eq!(sign_extend_24(0xFF_FFFF), -1);
/// assert_eq!(sign_extend_24(0xAB00_0001), 1); // high garbage masked
/// ```
#[must_use]
pub const fn sign_extend_24(raw: u32) -> i32 {
    // Shift the 24-bit value to the top, then arithmetic-shift back down.
    ((raw << 8) as i32) >> 8
}

/// Returns `true` when `raw` sits at either extreme of the 24-bit range.
///
/// A converter pinned at [`RAW_MIN`] or [`RAW_MAX`] is the signature of a
/// broken or open bridge wire.
/// A single rail sample proves nothing, so check a run of them.
///
/// ```
/// use tamer::hx711::{is_saturated, RAW_MAX, RAW_MIN};
///
/// assert!(is_saturated(RAW_MAX));
/// assert!(!is_saturated(0));
/// assert!(is_saturated(RAW_MIN));
/// ```
#[must_use]
pub const fn is_saturated(raw: i32) -> bool {
    raw <= RAW_MIN || raw >= RAW_MAX
}

/// A usable scale is a normal number: finite, non-zero and not subnormal.
///
/// A subnormal scale makes [`Calibration::grams`] overflow to infinity.
/// Negative scales are legal (an inverted bridge).
const fn valid_scale(counts_per_gram: f32) -> bool {
    counts_per_gram.is_normal()
}

/// Why a calibration value was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationError {
    /// The scale (counts per gram) is zero, subnormal, NaN or infinite, or a tared reading cannot define a normal one.
    InvalidScale,
    /// The temperature drift is NaN or infinite.
    InvalidDrift,
    /// The known mass is not a positive finite number of grams.
    InvalidMass,
}

impl core::fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidScale => write!(
                f,
                "scale must be a normal number: finite, non-zero, not subnormal"
            ),
            Self::InvalidDrift => write!(f, "temperature drift must be finite"),
            Self::InvalidMass => write!(f, "known mass must be positive and finite"),
        }
    }
}

/// Tare offset, scale and temperature compensation of one load-cell setup.
///
/// Fields are private so the invariants hold: the scale is a normal number
/// (finite, non-zero, not subnormal) and the drift is finite.
/// Build one with [`new`](Self::new) or restore it with [`from_bytes`](Self::from_bytes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    tare_offset: i32,
    counts_per_gram: f32,
    reference_deci_c: i16,
    drift_g_per_c: f32,
}

impl Calibration {
    /// Creates a calibration.
    ///
    /// `reference_deci_c` is the temperature at which the scale was calibrated,
    /// in tenths of a degree Celsius.
    /// `drift_g_per_c` is the apparent weight change per degree above the reference.
    ///
    /// # Errors
    ///
    /// [`CalibrationError::InvalidScale`] if `counts_per_gram` is not a normal number
    /// (zero, subnormal, NaN or infinite).
    /// [`CalibrationError::InvalidDrift`] if `drift_g_per_c` is not finite.
    pub const fn new(
        tare_offset: i32,
        counts_per_gram: f32,
        reference_deci_c: i16,
        drift_g_per_c: f32,
    ) -> Result<Self, CalibrationError> {
        if !valid_scale(counts_per_gram) {
            return Err(CalibrationError::InvalidScale);
        }
        if !drift_g_per_c.is_finite() {
            return Err(CalibrationError::InvalidDrift);
        }
        Ok(Self {
            tare_offset,
            counts_per_gram,
            reference_deci_c,
            drift_g_per_c,
        })
    }

    /// A placeholder until calibrated: no tare, one count per gram, no drift.
    ///
    /// This is not a real scale; readings are raw counts labelled as grams.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            tare_offset: 0,
            counts_per_gram: 1.0,
            reference_deci_c: 0,
            drift_g_per_c: 0.0,
        }
    }

    /// Returns the raw count of the empty scale.
    #[must_use]
    pub const fn tare_offset(&self) -> i32 {
        self.tare_offset
    }

    /// Returns the number of counts per gram.
    #[must_use]
    pub const fn counts_per_gram(&self) -> f32 {
        self.counts_per_gram
    }

    /// Returns the calibration temperature in tenths of a degree Celsius.
    #[must_use]
    pub const fn reference_deci_c(&self) -> i16 {
        self.reference_deci_c
    }

    /// Returns the drift in grams per degree Celsius.
    #[must_use]
    pub const fn drift_g_per_c(&self) -> f32 {
        self.drift_g_per_c
    }

    /// Subtracts the tare offset from `raw`, saturating at the `i32` limits.
    #[must_use]
    pub const fn tared(&self, raw: i32) -> i32 {
        raw.saturating_sub(self.tare_offset)
    }

    /// Converts a tared count to grams.
    ///
    /// A very small normal scale can still overflow a full-range count to infinity.
    #[must_use]
    pub const fn grams(&self, tared: i32) -> f32 {
        tared as f32 / self.counts_per_gram
    }

    /// Removes the temperature drift from `grams` measured at `temp_c` degrees Celsius.
    #[must_use]
    pub const fn compensate(&self, grams: f32, temp_c: f32) -> f32 {
        grams - self.drift_g_per_c * (temp_c - self.reference_deci_c as f32 / 10.0)
    }

    /// Serializes to the 16-byte layout in the [module docs](crate::hx711#calibration-byte-layout).
    #[must_use]
    pub const fn to_bytes(&self) -> [u8; 16] {
        let tare = self.tare_offset.to_le_bytes();
        let scale = self.counts_per_gram.to_le_bytes();
        let reference = self.reference_deci_c.to_le_bytes();
        let drift = self.drift_g_per_c.to_le_bytes();
        [
            CALIBRATION_MAGIC,
            CALIBRATION_VERSION,
            tare[0],
            tare[1],
            tare[2],
            tare[3],
            scale[0],
            scale[1],
            scale[2],
            scale[3],
            reference[0],
            reference[1],
            drift[0],
            drift[1],
            drift[2],
            drift[3],
        ]
    }

    /// Restores a calibration from [`to_bytes`](Self::to_bytes) output.
    ///
    /// Returns `None` on a wrong magic or version, or when the content would
    /// be rejected by [`new`](Self::new).
    #[must_use]
    pub const fn from_bytes(bytes: &[u8; 16]) -> Option<Self> {
        if bytes[0] != CALIBRATION_MAGIC || bytes[1] != CALIBRATION_VERSION {
            return None;
        }
        let tare = i32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        let scale = f32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]);
        let reference = i16::from_le_bytes([bytes[10], bytes[11]]);
        let drift = f32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        match Self::new(tare, scale, reference, drift) {
            Ok(calibration) => Some(calibration),
            Err(_) => None,
        }
    }
}

/// The weigh pipeline: tare, window average, scale, temperature compensation.
///
/// `N` is the averaging window in samples; `N = 1` disables averaging.
/// `N` must be greater than zero (see [`SignedSlidingAverage`]).
#[derive(Debug, Clone)]
pub struct LoadCellScale<const N: usize> {
    calibration: Calibration,
    window: SignedSlidingAverage<N>,
}

impl<const N: usize> LoadCellScale<N> {
    /// Creates a scale from a calibration, with an empty window.
    #[must_use]
    pub const fn new(calibration: Calibration) -> Self {
        Self {
            calibration,
            window: SignedSlidingAverage::new(),
        }
    }

    /// Feeds one raw reading and returns the weight in grams.
    ///
    /// The tared reading goes through the window, then the scale.
    /// Temperature compensation applies only when `temp_c` is `Some` and finite.
    pub fn weigh(&mut self, raw: i32, temp_c: Option<f32>) -> f32 {
        let tared = self.calibration.tared(raw);
        let averaged = self.window.push(tared);
        let grams = self.calibration.grams(averaged);
        match temp_c {
            Some(t) if t.is_finite() => self.calibration.compensate(grams, t),
            _ => grams,
        }
    }

    /// Takes `raw` as the empty-scale reading and clears the window.
    ///
    /// The other calibration fields are kept.
    /// Pass [`averaged_raw`](Self::averaged_raw) once [`window_is_full`](Self::window_is_full).
    pub fn tare(&mut self, raw: i32) {
        self.calibration.tare_offset = raw;
        self.window.reset();
    }

    /// Sets the scale from a raw reading taken with `known_grams` on the platform.
    ///
    /// Call [`tare`](Self::tare) first, then pass [`averaged_raw`](Self::averaged_raw)
    /// once [`window_is_full`](Self::window_is_full).
    ///
    /// # Errors
    ///
    /// [`CalibrationError::InvalidMass`] if `known_grams` is not positive and finite.
    /// [`CalibrationError::InvalidScale`] if the tared reading is zero or the
    /// resulting scale is not a normal number (for example a tiny count over a huge mass).
    /// The calibration and the window are untouched on error.
    pub fn calibrate(&mut self, raw: i32, known_grams: f32) -> Result<(), CalibrationError> {
        if !known_grams.is_finite() || known_grams <= 0.0 {
            return Err(CalibrationError::InvalidMass);
        }
        let tared = self.calibration.tared(raw);
        if tared == 0 {
            return Err(CalibrationError::InvalidScale);
        }
        let new_scale = tared as f32 / known_grams;
        if !valid_scale(new_scale) {
            return Err(CalibrationError::InvalidScale);
        }
        self.calibration.counts_per_gram = new_scale;
        Ok(())
    }

    /// Sets the temperature compensation.
    ///
    /// # Errors
    ///
    /// [`CalibrationError::InvalidDrift`] if `drift_g_per_c` is not finite.
    /// The state is untouched on error.
    pub fn set_temperature_compensation(
        &mut self,
        reference_deci_c: i16,
        drift_g_per_c: f32,
    ) -> Result<(), CalibrationError> {
        if !drift_g_per_c.is_finite() {
            return Err(CalibrationError::InvalidDrift);
        }
        self.calibration.reference_deci_c = reference_deci_c;
        self.calibration.drift_g_per_c = drift_g_per_c;
        Ok(())
    }

    /// Returns a copy of the current calibration, e.g. to persist it.
    #[must_use]
    pub const fn calibration(&self) -> Calibration {
        self.calibration
    }

    /// Returns the window average as a raw count, or `None` while the window is empty.
    ///
    /// The window holds tared samples, so the tare offset is added back.
    /// The result is exact up to the average's truncation.
    ///
    /// Use it for the tare and span steps:
    /// weigh `N` samples on the empty platform, then [`tare`](Self::tare) with this value;
    /// place the mass, weigh `N` samples, then [`calibrate`](Self::calibrate) with this value and the mass.
    /// The [`weigh`](Self::weigh) results during these steps are discarded.
    #[must_use]
    pub fn averaged_raw(&self) -> Option<i32> {
        if self.window.is_empty() {
            return None;
        }
        Some(
            self.window
                .average()
                .saturating_add(self.calibration.tare_offset()),
        )
    }

    /// Returns the number of samples currently in the window.
    #[must_use]
    pub const fn window_len(&self) -> usize {
        self.window.len()
    }

    /// Returns `true` once the window holds `N` samples.
    #[must_use]
    pub const fn window_is_full(&self) -> bool {
        self.window.is_full()
    }
}

/// Masks interrupts around the HX711 pulse train.
///
/// Contract for implementors:
///
/// - `masked` runs `f` with interrupts masked on the calling core.
///   A mutex-style lock is not enough: an interrupt could still preempt the
///   pulse train and hold PD_SCK high past [`POWER_DOWN_US`].
/// - `f` runs exactly once.
/// - `f` must not log, allocate or block.
///
/// Callers keep `f` to the pulse train, about 100 to 135 us.
pub trait InterruptMask {
    /// Runs `f` with interrupts masked and returns its result.
    fn masked<R>(&self, f: impl FnOnce() -> R) -> R;
}

/// An [`InterruptMask`] that masks nothing, for host tests and for targets without interrupts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoopInterruptMask;

impl InterruptMask for NoopInterruptMask {
    fn masked<R>(&self, f: impl FnOnce() -> R) -> R {
        f()
    }
}

// ─── Hx711 driver ────────────────────────────────────────────────────────────

/// PD_SCK high time used by [`Hx711::power_down`]: [`POWER_DOWN_US`] plus margin.
#[cfg(feature = "hal")]
const POWER_DOWN_HOLD_US: u32 = 70;

/// Error from the [`Hx711`] driver, generic over the DOUT and PD_SCK pin errors.
#[cfg(feature = "hal")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<ED, ES> {
    /// Reading the DOUT pin failed.
    Dout(ED),
    /// Driving the PD_SCK pin failed.
    Sck(ES),
    /// No conversion is ready: DOUT is still high.
    NotReady,
}

#[cfg(feature = "hal")]
impl<ED: core::fmt::Debug, ES: core::fmt::Debug> core::fmt::Display for Error<ED, ES> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Dout(e) => write!(f, "HX711 DOUT pin error: {e:?}"),
            Self::Sck(e) => write!(f, "HX711 PD_SCK pin error: {e:?}"),
            Self::NotReady => write!(f, "HX711 not ready: DOUT is still high"),
        }
    }
}

/// Bit-banged HX711 driver over an `embedded_hal` input (DOUT) and output (PD_SCK) pin.
///
/// The driver owns both pins; hand them back with [`release`](Self::release).
/// It holds no decode logic beyond [`sign_extend_24`]; the tare / scale pipeline is
/// [`LoadCellScale`].
/// The pulse train runs inside [`InterruptMask::masked`] so an interrupt cannot hold
/// PD_SCK high past [`POWER_DOWN_US`] and power the chip down mid-read (ADR-016).
///
/// Experimental: not yet verified on hardware.
///
/// # Example
///
/// ```
/// use tamer::hx711::{Gain, Hx711, NoopInterruptMask};
/// use tamer::mock::{MockInputPin, MockOutputPin, NoopDelay};
///
/// // DOUT held low reads as an all-zero word, then the gain pulses.
/// let mut adc = Hx711::new(MockInputPin::new(false), MockOutputPin::new(false));
/// adc.power_up().unwrap();
/// let raw = adc
///     .read(&mut NoopDelay, &NoopInterruptMask, Gain::A128)
///     .unwrap();
/// assert_eq!(raw, 0);
/// let (_dout, sck) = adc.release();
/// assert_eq!(sck.rising_edges(), 25);
/// ```
#[cfg(feature = "hal")]
#[derive(Debug)]
pub struct Hx711<Dout, Sck> {
    dout: Dout,
    sck: Sck,
}

/// Shorthand for the driver's error type over a pin pair.
#[cfg(feature = "hal")]
type DriverError<Dout, Sck> = Error<
    <Dout as embedded_hal::digital::ErrorType>::Error,
    <Sck as embedded_hal::digital::ErrorType>::Error,
>;

#[cfg(feature = "hal")]
impl<Dout, Sck> Hx711<Dout, Sck>
where
    Dout: embedded_hal::digital::InputPin,
    Sck: embedded_hal::digital::OutputPin,
{
    /// Creates a driver over the two pins; touches no pin.
    #[must_use]
    pub const fn new(dout: Dout, sck: Sck) -> Self {
        Self { dout, sck }
    }

    /// Powers the chip up (or keeps it running) by driving PD_SCK low.
    ///
    /// The first conversion after power-up takes [`SETTLE_MS_10HZ`] or [`SETTLE_MS_80HZ`]
    /// depending on the RATE pin; discard it.
    ///
    /// # Errors
    ///
    /// [`Error::Sck`] if the PD_SCK pin fails.
    pub fn power_up(&mut self) -> Result<(), DriverError<Dout, Sck>> {
        self.sck.set_low().map_err(Error::Sck)
    }

    /// Powers the chip down: PD_SCK high for longer than [`POWER_DOWN_US`].
    ///
    /// Leaves PD_SCK high; call [`power_up`](Self::power_up) to resume.
    ///
    /// # Errors
    ///
    /// [`Error::Sck`] if the PD_SCK pin fails.
    pub fn power_down<D: embedded_hal::delay::DelayNs>(
        &mut self,
        delay: &mut D,
    ) -> Result<(), DriverError<Dout, Sck>> {
        self.sck.set_low().map_err(Error::Sck)?;
        self.sck.set_high().map_err(Error::Sck)?;
        delay.delay_us(POWER_DOWN_HOLD_US);
        Ok(())
    }

    /// Returns `true` when a conversion is ready (DOUT low).
    ///
    /// # Errors
    ///
    /// [`Error::Dout`] if the DOUT pin fails.
    pub fn is_ready(&mut self) -> Result<bool, DriverError<Dout, Sck>> {
        self.dout.is_low().map_err(Error::Dout)
    }

    /// Reads one conversion and selects `gain` for the next one.
    ///
    /// Clocks 24 data bits MSB first, then the gain's extra pulses, inside
    /// `mask.masked`.
    /// After a gain change the next sample still uses the old gain; discard it.
    ///
    /// If a pin fails inside the pulse train, the driver drives PD_SCK low
    /// and then completes the remaining pulses with the clock alone, still
    /// inside the mask.
    /// The fault therefore cannot hold the line high past [`POWER_DOWN_US`],
    /// and the chip ends the transfer with the requested gain, so DOUT
    /// returns high and the next conversion proceeds normally.
    /// The interrupted sample is lost and the error is returned.
    /// After an [`Error::Sck`] the recovery cannot know whether the clock
    /// line moved, so the pulse count may be off by one; power-cycle the
    /// chip with [`power_down`](Self::power_down) and
    /// [`power_up`](Self::power_up) before trusting readings again.
    ///
    /// # Errors
    ///
    /// - [`Error::NotReady`] if DOUT is high; no pulse is issued.
    /// - [`Error::Dout`] / [`Error::Sck`] if a pin fails; the recovery
    ///   (drop the clock, finish the train) is best effort and its own
    ///   failures are not reported.
    pub fn read<D: embedded_hal::delay::DelayNs, M: InterruptMask>(
        &mut self,
        delay: &mut D,
        mask: &M,
        gain: Gain,
    ) -> Result<i32, DriverError<Dout, Sck>> {
        if !self.is_ready()? {
            return Err(Error::NotReady);
        }
        let dout = &mut self.dout;
        let sck = &mut self.sck;
        let value = mask.masked(|| {
            let mut issued = 0u8;
            let clocked = Self::clock_out(dout, sck, delay, gain, &mut issued);
            if clocked.is_err() {
                // The fault left PD_SCK at an unknown level and the chip in the
                // middle of a transfer. Drop the clock so it cannot sit high past
                // POWER_DOWN_US, then finish the train clock-only so the HX711
                // ends the transfer with the requested gain and DOUT returns
                // high. Best effort: the original error is the one reported.
                let _ = sck.set_low();
                delay.delay_us(PULSE_US);
                Self::resync(sck, delay, gain.total_pulses().saturating_sub(issued));
            }
            clocked
        })?;
        Ok(sign_extend_24(value))
    }

    /// The pulse train of one conversion: 24 data bits MSB first, then the
    /// gain's extra pulses.
    /// Runs inside the caller's mask; a pin error returns at once with
    /// PD_SCK at whatever level the failing call left it, and `issued`
    /// holds the number of rising edges the chip has seen so far.
    fn clock_out<D: embedded_hal::delay::DelayNs>(
        dout: &mut Dout,
        sck: &mut Sck,
        delay: &mut D,
        gain: Gain,
        issued: &mut u8,
    ) -> Result<u32, DriverError<Dout, Sck>> {
        let mut value = 0u32;
        for _ in 0..24 {
            sck.set_high().map_err(Error::Sck)?;
            *issued += 1;
            delay.delay_us(PULSE_US);
            let bit = dout.is_high().map_err(Error::Dout)?;
            sck.set_low().map_err(Error::Sck)?;
            delay.delay_us(PULSE_US);
            value = (value << 1) | u32::from(bit);
        }
        for _ in 0..gain.extra_pulses() {
            sck.set_high().map_err(Error::Sck)?;
            *issued += 1;
            delay.delay_us(PULSE_US);
            sck.set_low().map_err(Error::Sck)?;
            delay.delay_us(PULSE_US);
        }
        Ok(value)
    }

    /// Issues `pulses` clock-only pulses to end an interrupted transfer.
    /// Pin errors are ignored: this runs after a fault, as a best effort.
    fn resync<D: embedded_hal::delay::DelayNs>(sck: &mut Sck, delay: &mut D, pulses: u8) {
        for _ in 0..pulses {
            let _ = sck.set_high();
            delay.delay_us(PULSE_US);
            let _ = sck.set_low();
            delay.delay_us(PULSE_US);
        }
    }

    /// Waits up to `max_polls` times, [`POLL_INTERVAL_US`] apart, for DOUT to drop, then reads.
    ///
    /// DOUT is checked `max_polls + 1` times, so `0` is a single non-blocking attempt.
    ///
    /// The CPU is held for about `max_polls` x 100 us.
    /// 2 000 polls is 200 ms, and one 10 Hz conversion needs about 1 000.
    /// Keep the budget well under the ESP-IDF task watchdog (default 5 s, roughly 50 000 polls).
    ///
    /// # Errors
    ///
    /// - [`Error::NotReady`] if DOUT is still high after `max_polls` waits.
    /// - [`Error::Dout`] / [`Error::Sck`] if a pin fails.
    pub fn read_blocking<D: embedded_hal::delay::DelayNs, M: InterruptMask>(
        &mut self,
        delay: &mut D,
        mask: &M,
        gain: Gain,
        max_polls: u32,
    ) -> Result<i32, DriverError<Dout, Sck>> {
        for _ in 0..max_polls {
            if self.is_ready()? {
                return self.read(delay, mask, gain);
            }
            delay.delay_us(POLL_INTERVAL_US);
        }
        self.read(delay, mask, gain)
    }

    /// Consumes the driver and returns the `(dout, sck)` pins.
    #[must_use]
    pub fn release(self) -> (Dout, Sck) {
        (self.dout, self.sck)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(actual: f32, expected: f32, tol: f32) {
        assert!(
            (actual - expected).abs() <= tol,
            "expected {expected} +/- {tol}, got {actual}"
        );
    }

    // ── pure ────────────────────────────────────────────────────────────

    #[test]
    fn sign_extension_fixtures() {
        assert_eq!(sign_extend_24(0), 0);
        assert_eq!(sign_extend_24(0x12_3456), 0x12_3456);
        assert_eq!(sign_extend_24(0x7F_FFFF), RAW_MAX);
        assert_eq!(sign_extend_24(0x80_0000), RAW_MIN);
        assert_eq!(sign_extend_24(0xFF_FFFF), -1);
        assert_eq!(sign_extend_24(0xFF12_3456), 0x12_3456);
        assert_eq!(sign_extend_24(0x0180_0000), RAW_MIN);
    }

    #[test]
    fn pulse_counts_per_gain() {
        assert_eq!(Gain::A128.extra_pulses(), 1);
        assert_eq!(Gain::B32.extra_pulses(), 2);
        assert_eq!(Gain::A64.extra_pulses(), 3);
        assert_eq!(Gain::A128.total_pulses(), 25);
        assert_eq!(Gain::B32.total_pulses(), 26);
        assert_eq!(Gain::A64.total_pulses(), 27);
    }

    #[test]
    fn constants_match_datasheet() {
        assert_eq!(RAW_MIN, -(1 << 23));
        assert_eq!(RAW_MAX, (1 << 23) - 1);
        assert_eq!(POWER_DOWN_US, 60);
        assert_eq!(SETTLE_MS_10HZ, 400);
        assert_eq!(SETTLE_MS_80HZ, 50);
        assert_eq!(POLL_INTERVAL_US, 100);
        assert_eq!(PULSE_US, 2);
    }

    #[test]
    fn saturation_detects_both_extremes() {
        assert!(is_saturated(RAW_MIN));
        assert!(is_saturated(RAW_MAX));
        assert!(!is_saturated(RAW_MIN + 1));
        assert!(!is_saturated(0));
        assert!(!is_saturated(RAW_MAX - 1));
    }

    #[test]
    fn subnormal_scales_rejected_by_new_and_from_bytes() {
        for bad in [1e-45_f32, f32::MIN_POSITIVE / 2.0, -1e-45_f32] {
            assert_eq!(
                Calibration::new(0, bad, 0, 0.0),
                Err(CalibrationError::InvalidScale)
            );
            let mut b = Calibration::new(1, 2.0, 3, 4.0).unwrap().to_bytes();
            b[6..10].copy_from_slice(&bad.to_le_bytes());
            assert_eq!(Calibration::from_bytes(&b), None);
        }
        assert!(Calibration::new(0, f32::MIN_POSITIVE, 0, 0.0).is_ok());
        assert!(Calibration::new(0, -2.0, 0, 0.0).is_ok());
    }

    #[test]
    fn new_validates_scale_and_drift() {
        assert!(Calibration::new(0, 1.0, 0, 0.0).is_ok());
        for bad in [0.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                Calibration::new(0, bad, 0, 0.0),
                Err(CalibrationError::InvalidScale)
            );
        }
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                Calibration::new(0, 1.0, 0, bad),
                Err(CalibrationError::InvalidDrift)
            );
        }
    }

    #[test]
    fn identity_is_neutral() {
        let c = Calibration::identity();
        assert_eq!(c.tare_offset(), 0);
        assert_eq!(c.counts_per_gram(), 1.0);
        assert_eq!(c.reference_deci_c(), 0);
        assert_eq!(c.drift_g_per_c(), 0.0);
        assert_eq!(c.grams(c.tared(1234)), 1234.0);
    }

    #[test]
    fn tared_saturates() {
        let c = Calibration::new(i32::MIN, 1.0, 0, 0.0).unwrap();
        assert_eq!(c.tared(i32::MAX), i32::MAX);
        let c = Calibration::new(i32::MAX, 1.0, 0, 0.0).unwrap();
        assert_eq!(c.tared(i32::MIN), i32::MIN);
    }

    #[test]
    fn drift_sign() {
        // Reference 20.0 C, 2 g/C: reading at 30 C is 20 g too high.
        let c = Calibration::new(0, 1.0, 200, 2.0).unwrap();
        approx(c.compensate(0.0, 30.0), -20.0, 1e-4);
        approx(c.compensate(0.0, 10.0), 20.0, 1e-4);
        approx(c.compensate(100.0, 20.0), 100.0, 1e-4);
    }

    #[test]
    fn codec_round_trip_and_golden_bytes() {
        let c = Calibration::new(8_000_000, 21.0, 200, 2.0).unwrap();
        let bytes = c.to_bytes();
        assert_eq!(
            bytes,
            [
                0xC1, 0x01, // magic, version
                0x00, 0x12, 0x7A, 0x00, // 8_000_000
                0x00, 0x00, 0xA8, 0x41, // 21.0f32
                0xC8, 0x00, // 200
                0x00, 0x00, 0x00, 0x40, // 2.0f32
            ]
        );
        assert_eq!(Calibration::from_bytes(&bytes), Some(c));

        let n = Calibration::new(-5, -0.25, -40, -1.5).unwrap();
        assert_eq!(Calibration::from_bytes(&n.to_bytes()), Some(n));
    }

    #[test]
    fn codec_rejects_bad_input() {
        let good = Calibration::new(1, 2.0, 3, 4.0).unwrap().to_bytes();
        let mut b = good;
        b[0] = 0;
        assert_eq!(Calibration::from_bytes(&b), None);
        let mut b = good;
        b[1] = CALIBRATION_VERSION + 1;
        assert_eq!(Calibration::from_bytes(&b), None);
        let mut b = good;
        b[6..10].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(Calibration::from_bytes(&b), None);
        let mut b = good;
        b[6..10].copy_from_slice(&0.0f32.to_le_bytes());
        assert_eq!(Calibration::from_bytes(&b), None);
        let mut b = good;
        b[12..16].copy_from_slice(&f32::INFINITY.to_le_bytes());
        assert_eq!(Calibration::from_bytes(&b), None);
    }

    #[test]
    fn error_display() {
        extern crate std;
        use std::string::ToString;
        assert!(!CalibrationError::InvalidScale.to_string().is_empty());
        assert!(!CalibrationError::InvalidDrift.to_string().is_empty());
        assert!(!CalibrationError::InvalidMass.to_string().is_empty());
    }

    #[test]
    fn noop_mask_runs_closure_once() {
        let mut calls = 0;
        let out = NoopInterruptMask.masked(|| {
            calls += 1;
            7
        });
        assert_eq!(out, 7);
        assert_eq!(calls, 1);
    }

    // ── pipeline ────────────────────────────────────────────────────────

    #[test]
    fn donor_five_kg_case() {
        let mut scale = LoadCellScale::<1>::new(Calibration::identity());
        scale.tare(8_000_000);
        scale.calibrate(8_210_000, 10_000.0).unwrap();
        approx(scale.weigh(8_105_000, None), 5_000.0, 1.0);
    }

    #[test]
    fn window_averages_readings() {
        let mut scale = LoadCellScale::<2>::new(Calibration::new(100, 1.0, 0, 0.0).unwrap());
        approx(scale.weigh(110, None), 10.0, 1e-4);
        approx(scale.weigh(130, None), 20.0, 1e-4);
    }

    #[test]
    fn retare_clears_window() {
        let mut scale = LoadCellScale::<4>::new(Calibration::identity());
        let _ = scale.weigh(1000, None);
        let _ = scale.weigh(1000, None);
        scale.tare(500);
        assert_eq!(scale.calibration().tare_offset(), 500);
        // A stale window would average in the old 1000s.
        approx(scale.weigh(500, None), 0.0, 1e-4);
    }

    #[test]
    fn tare_keeps_other_fields() {
        let mut scale = LoadCellScale::<2>::new(Calibration::new(0, 21.0, 200, 2.0).unwrap());
        scale.tare(77);
        let c = scale.calibration();
        assert_eq!(c.counts_per_gram(), 21.0);
        assert_eq!(c.reference_deci_c(), 200);
        assert_eq!(c.drift_g_per_c(), 2.0);
    }

    #[test]
    fn failed_calibrate_leaves_state_untouched() {
        let mut scale = LoadCellScale::<2>::new(Calibration::new(100, 2.0, 0, 0.0).unwrap());
        let _ = scale.weigh(120, None);
        let before = scale.calibration();
        assert_eq!(
            scale.calibrate(200, 0.0),
            Err(CalibrationError::InvalidMass)
        );
        assert_eq!(
            scale.calibrate(200, -1.0),
            Err(CalibrationError::InvalidMass)
        );
        assert_eq!(
            scale.calibrate(200, f32::NAN),
            Err(CalibrationError::InvalidMass)
        );
        assert_eq!(
            scale.calibrate(100, 10.0),
            Err(CalibrationError::InvalidScale)
        );
        assert_eq!(scale.calibration(), before);
        // The window survived too: (20 + 40) / 2 = 30 counts at 2 counts/g.
        approx(scale.weigh(140, None), 15.0, 1e-4);
    }

    #[test]
    fn calibrate_rejects_subnormal_scale_and_keeps_state() {
        let mut scale = LoadCellScale::<2>::new(Calibration::new(0, 2.0, 0, 0.0).unwrap());
        let _ = scale.weigh(10, None);
        let before = scale.calibration();
        // 1 / f32::MAX and 2 / f32::MAX are both subnormal.
        for tared in [1, 2] {
            assert_eq!(
                scale.calibrate(tared, f32::MAX),
                Err(CalibrationError::InvalidScale)
            );
        }
        assert_eq!(scale.calibration(), before);
        assert_eq!(scale.window_len(), 1);
    }

    #[test]
    fn averaged_raw_tracks_window_and_tare() {
        let mut scale = LoadCellScale::<4>::new(Calibration::new(100, 1.0, 0, 0.0).unwrap());
        assert_eq!(scale.averaged_raw(), None);
        assert_eq!(scale.window_len(), 0);
        assert!(!scale.window_is_full());
        let _ = scale.weigh(110, None);
        let _ = scale.weigh(130, None);
        assert_eq!(scale.averaged_raw(), Some(120));
        assert_eq!(scale.window_len(), 2);
        assert!(!scale.window_is_full());
        let _ = scale.weigh(120, None);
        let _ = scale.weigh(120, None);
        assert_eq!(scale.window_len(), 4);
        assert!(scale.window_is_full());
        assert_eq!(scale.averaged_raw(), Some(120));
        scale.tare(120);
        assert_eq!(scale.averaged_raw(), None);
        assert_eq!(scale.window_len(), 0);
        assert!(!scale.window_is_full());
    }

    #[test]
    fn temperature_compensation_applies_only_for_finite_temperature() {
        let mut scale = LoadCellScale::<1>::new(Calibration::identity());
        scale.set_temperature_compensation(200, 2.0).unwrap();
        approx(scale.weigh(100, Some(30.0)), 80.0, 1e-4);
        approx(scale.weigh(100, None), 100.0, 1e-4);
        approx(scale.weigh(100, Some(f32::NAN)), 100.0, 1e-4);
        approx(scale.weigh(100, Some(f32::INFINITY)), 100.0, 1e-4);
    }

    #[test]
    fn invalid_drift_rejected_and_state_kept() {
        let mut scale = LoadCellScale::<1>::new(Calibration::identity());
        assert_eq!(
            scale.set_temperature_compensation(200, f32::NAN),
            Err(CalibrationError::InvalidDrift)
        );
        assert_eq!(scale.calibration(), Calibration::identity());
    }
}

#[cfg(all(test, feature = "hal"))]
mod hal_tests {
    extern crate std;

    use super::*;
    use embedded_hal::delay::DelayNs;
    use embedded_hal::digital::{ErrorKind, ErrorType, InputPin, OutputPin};
    use std::cell::Cell;
    use std::rc::Rc;

    /// State shared by the scripted DOUT, the recording SCK and the counting mask.
    struct Bus {
        word: u32,
        busy_polls: Cell<u32>,
        edges: Cell<u32>,
        depth: Cell<u32>,
        masked_calls: Cell<u32>,
        unmasked_edges: Cell<u32>,
        sck_high: Cell<bool>,
    }

    impl Bus {
        fn new(word: u32) -> Rc<Self> {
            Rc::new(Self {
                word,
                busy_polls: Cell::new(0),
                edges: Cell::new(0),
                depth: Cell::new(0),
                masked_calls: Cell::new(0),
                unmasked_edges: Cell::new(0),
                sck_high: Cell::new(false),
            })
        }
    }

    /// DOUT that shifts one bit of a 24-bit word out per PD_SCK rising edge.
    struct ScriptedDout(Rc<Bus>);

    impl ErrorType for ScriptedDout {
        type Error = core::convert::Infallible;
    }

    impl InputPin for ScriptedDout {
        fn is_high(&mut self) -> Result<bool, Self::Error> {
            let edges = self.0.edges.get();
            Ok(match edges {
                0 => {
                    let left = self.0.busy_polls.get();
                    if left > 0 {
                        self.0.busy_polls.set(left - 1);
                        true
                    } else {
                        false
                    }
                }
                1..=24 => (self.0.word >> (24 - edges)) & 1 == 1,
                _ => true,
            })
        }

        fn is_low(&mut self) -> Result<bool, Self::Error> {
            self.is_high().map(|h| !h)
        }
    }

    struct SharedSck(Rc<Bus>);

    impl ErrorType for SharedSck {
        type Error = core::convert::Infallible;
    }

    impl OutputPin for SharedSck {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.0.sck_high.set(false);
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            if !self.0.sck_high.get() {
                self.0.edges.set(self.0.edges.get() + 1);
                if self.0.depth.get() != 1 {
                    self.0.unmasked_edges.set(self.0.unmasked_edges.get() + 1);
                }
            }
            self.0.sck_high.set(true);
            Ok(())
        }
    }

    struct CountingMask(Rc<Bus>);

    impl InterruptMask for CountingMask {
        fn masked<R>(&self, f: impl FnOnce() -> R) -> R {
            self.0.depth.set(self.0.depth.get() + 1);
            self.0.masked_calls.set(self.0.masked_calls.get() + 1);
            let out = f();
            self.0.depth.set(self.0.depth.get() - 1);
            out
        }
    }

    #[derive(Default)]
    struct CountingDelay {
        polls: u32,
    }

    impl DelayNs for CountingDelay {
        fn delay_ns(&mut self, ns: u32) {
            if ns == POLL_INTERVAL_US * 1_000 {
                self.polls += 1;
            }
        }
    }

    /// Pin of either direction whose every operation fails.
    struct Failing(&'static str);

    /// Error carrying a `&'static str` tag, local to satisfy the orphan rule.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Fault(&'static str);

    impl embedded_hal::digital::Error for Fault {
        fn kind(&self) -> ErrorKind {
            ErrorKind::Other
        }
    }

    impl ErrorType for Failing {
        type Error = Fault;
    }

    impl InputPin for Failing {
        fn is_high(&mut self) -> Result<bool, Self::Error> {
            Err(Fault(self.0))
        }

        fn is_low(&mut self) -> Result<bool, Self::Error> {
            Err(Fault(self.0))
        }
    }

    impl OutputPin for Failing {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            Err(Fault(self.0))
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            Err(Fault(self.0))
        }
    }

    fn driver(word: u32) -> (Hx711<ScriptedDout, SharedSck>, Rc<Bus>) {
        let bus = Bus::new(word);
        (
            Hx711::new(ScriptedDout(bus.clone()), SharedSck(bus.clone())),
            bus,
        )
    }

    #[test]
    fn reads_bits_msb_first_and_pulses_for_gain_a128() {
        let (mut adc, bus) = driver(0x12_3456);
        let raw = adc
            .read(&mut NoopDelayLocal, &NoopInterruptMask, Gain::A128)
            .unwrap();
        assert_eq!(raw, 0x12_3456);
        assert_eq!(bus.edges.get(), 25);
    }

    #[test]
    fn sign_extends_and_pulses_for_gain_b32_and_a64() {
        let (mut adc, bus) = driver(0x80_0000);
        let raw = adc
            .read(&mut NoopDelayLocal, &NoopInterruptMask, Gain::B32)
            .unwrap();
        assert_eq!(raw, RAW_MIN);
        assert_eq!(bus.edges.get(), 26);

        let (mut adc, bus) = driver(0x80_0000);
        let raw = adc
            .read(&mut NoopDelayLocal, &NoopInterruptMask, Gain::A64)
            .unwrap();
        assert_eq!(raw, RAW_MIN);
        assert_eq!(bus.edges.get(), 27);
    }

    #[test]
    fn full_scale_positive_word() {
        let (mut adc, _bus) = driver(0x7F_FFFF);
        let raw = adc
            .read(&mut NoopDelayLocal, &NoopInterruptMask, Gain::A128)
            .unwrap();
        assert_eq!(raw, RAW_MAX);
    }

    /// Delay that does nothing, local so the tests do not depend on the mock module.
    struct NoopDelayLocal;

    impl DelayNs for NoopDelayLocal {
        fn delay_ns(&mut self, _ns: u32) {}
    }

    #[test]
    fn not_ready_issues_no_pulse() {
        let (mut adc, bus) = driver(0x12_3456);
        bus.busy_polls.set(u32::MAX);
        let err = adc
            .read(&mut NoopDelayLocal, &NoopInterruptMask, Gain::A128)
            .unwrap_err();
        assert_eq!(err, Error::NotReady);
        assert_eq!(bus.edges.get(), 0);
        assert!(!adc.is_ready().unwrap());
    }

    /// DOUT that reports ready, then fails on the sample taken after the
    /// `fail_at`-th rising edge, with the clock recorder shared through `Bus`.
    struct FailingMidRead {
        bus: Rc<Bus>,
        fail_at: u32,
    }

    impl ErrorType for FailingMidRead {
        type Error = Fault;
    }

    impl InputPin for FailingMidRead {
        fn is_high(&mut self) -> Result<bool, Self::Error> {
            if self.bus.edges.get() == self.fail_at {
                Err(Fault("dout"))
            } else {
                Ok(false)
            }
        }

        fn is_low(&mut self) -> Result<bool, Self::Error> {
            self.is_high().map(|h| !h)
        }
    }

    #[test]
    fn pin_fault_mid_read_drops_the_clock() {
        let bus = Bus::new(0);
        let dout = FailingMidRead {
            bus: bus.clone(),
            fail_at: 5,
        };
        let mut adc = Hx711::new(dout, SharedSck(bus.clone()));
        let mask = CountingMask(bus.clone());
        let err = adc
            .read(&mut NoopDelayLocal, &mask, Gain::A128)
            .unwrap_err();
        assert_eq!(err, Error::Dout(Fault("dout")));
        assert_eq!(
            bus.edges.get(),
            25,
            "the train is completed clock-only so the chip ends the transfer"
        );
        assert!(
            !bus.sck_high.get(),
            "PD_SCK must be driven low again after a mid-read fault"
        );
        assert_eq!(
            bus.unmasked_edges.get(),
            0,
            "the recovery stays inside the mask"
        );
        assert_eq!(bus.depth.get(), 0, "the mask is released");
    }

    /// PD_SCK that fails exactly once, either on the `set_high` that would
    /// create edge `fail_edge` (the line does not move) or on the `set_low`
    /// that ends that edge (the line stays high); otherwise it is `SharedSck`.
    struct FaultingSck {
        bus: Rc<Bus>,
        fail_edge: u32,
        on_high: bool,
        fired: Cell<bool>,
    }

    impl ErrorType for FaultingSck {
        type Error = Fault;
    }

    impl OutputPin for FaultingSck {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            if !self.on_high && !self.fired.get() && self.bus.edges.get() == self.fail_edge {
                self.fired.set(true);
                return Err(Fault("sck"));
            }
            SharedSck(self.bus.clone())
                .set_low()
                .map_err(|never| match never {})
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            if self.on_high && !self.fired.get() && self.bus.edges.get() + 1 == self.fail_edge {
                self.fired.set(true);
                return Err(Fault("sck"));
            }
            SharedSck(self.bus.clone())
                .set_high()
                .map_err(|never| match never {})
        }
    }

    #[test]
    fn sck_fault_on_set_low_mid_read_resyncs() {
        let bus = Bus::new(0);
        let sck = FaultingSck {
            bus: bus.clone(),
            fail_edge: 5,
            on_high: false,
            fired: Cell::new(false),
        };
        let mut adc = Hx711::new(ScriptedDout(bus.clone()), sck);
        let err = adc
            .read(&mut NoopDelayLocal, &NoopInterruptMask, Gain::A128)
            .unwrap_err();
        assert_eq!(err, Error::Sck(Fault("sck")));
        assert_eq!(
            bus.edges.get(),
            25,
            "the counted edge is completed, then 20 more"
        );
        assert!(!bus.sck_high.get());
    }

    #[test]
    fn sck_fault_on_set_high_mid_read_resyncs() {
        let bus = Bus::new(0);
        let sck = FaultingSck {
            bus: bus.clone(),
            fail_edge: 5,
            on_high: true,
            fired: Cell::new(false),
        };
        let mut adc = Hx711::new(ScriptedDout(bus.clone()), sck);
        let err = adc
            .read(&mut NoopDelayLocal, &NoopInterruptMask, Gain::A128)
            .unwrap_err();
        assert_eq!(err, Error::Sck(Fault("sck")));
        assert_eq!(
            bus.edges.get(),
            25,
            "the uncounted edge is re-issued, 21 pulses in total"
        );
        assert!(!bus.sck_high.get());
    }

    #[test]
    fn pin_fault_mid_read_completes_the_gain_pulses() {
        let bus = Bus::new(0);
        let dout = FailingMidRead {
            bus: bus.clone(),
            fail_at: 5,
        };
        let mut adc = Hx711::new(dout, SharedSck(bus.clone()));
        let err = adc
            .read(&mut NoopDelayLocal, &NoopInterruptMask, Gain::B32)
            .unwrap_err();
        assert_eq!(err, Error::Dout(Fault("dout")));
        assert_eq!(bus.edges.get(), 26, "B32 needs 26 pulses in total");
        assert!(!bus.sck_high.get());
    }

    #[test]
    fn dout_error_maps_to_dout() {
        let mut adc = Hx711::new(Failing("dout"), crate::mock::MockOutputPin::new(false));
        assert_eq!(adc.is_ready(), Err(Error::Dout(Fault("dout"))));
        let err = adc
            .read(&mut NoopDelayLocal, &NoopInterruptMask, Gain::A128)
            .unwrap_err();
        assert_eq!(err, Error::Dout(Fault("dout")));
    }

    #[test]
    fn sck_error_maps_to_sck() {
        let mut adc = Hx711::new(crate::mock::MockInputPin::new(false), Failing("sck"));
        assert_eq!(adc.power_up(), Err(Error::Sck(Fault("sck"))));
        assert_eq!(
            adc.power_down(&mut NoopDelayLocal),
            Err(Error::Sck(Fault("sck")))
        );
        let err = adc
            .read(&mut NoopDelayLocal, &NoopInterruptMask, Gain::A128)
            .unwrap_err();
        assert_eq!(err, Error::Sck(Fault("sck")));
    }

    #[test]
    fn every_edge_is_masked_and_mask_runs_once() {
        let (mut adc, bus) = driver(0x00_00FF);
        let mask = CountingMask(bus.clone());
        adc.read(&mut NoopDelayLocal, &mask, Gain::A64).unwrap();
        assert_eq!(bus.masked_calls.get(), 1);
        assert_eq!(bus.edges.get(), 27);
        assert_eq!(bus.unmasked_edges.get(), 0);
        assert_eq!(bus.depth.get(), 0);
    }

    #[test]
    fn not_ready_does_not_enter_the_mask() {
        let (mut adc, bus) = driver(0);
        bus.busy_polls.set(u32::MAX);
        let mask = CountingMask(bus.clone());
        let _ = adc.read(&mut NoopDelayLocal, &mask, Gain::A128);
        assert_eq!(bus.masked_calls.get(), 0);
    }

    #[test]
    fn power_down_leaves_sck_high_and_power_up_low() {
        let (mut adc, bus) = driver(0);
        adc.power_down(&mut NoopDelayLocal).unwrap();
        assert!(bus.sck_high.get());
        adc.power_up().unwrap();
        assert!(!bus.sck_high.get());
    }

    #[test]
    fn power_down_holds_longer_than_threshold() {
        struct Total(u64);
        impl DelayNs for Total {
            fn delay_ns(&mut self, ns: u32) {
                self.0 += u64::from(ns);
            }
        }
        let (mut adc, _bus) = driver(0);
        let mut total = Total(0);
        adc.power_down(&mut total).unwrap();
        assert!(total.0 > u64::from(POWER_DOWN_US) * 1_000);
    }

    #[test]
    fn release_returns_the_pins() {
        let adc = Hx711::new(
            crate::mock::MockInputPin::new(true),
            crate::mock::MockOutputPin::new(true),
        );
        let (dout, sck) = adc.release();
        assert_eq!(dout, crate::mock::MockInputPin::new(true));
        assert!(sck.is_set_high());
    }

    #[test]
    fn read_blocking_gives_up_after_max_polls() {
        let (mut adc, bus) = driver(0x12_3456);
        bus.busy_polls.set(u32::MAX);
        let mut delay = CountingDelay::default();
        let err = adc
            .read_blocking(&mut delay, &NoopInterruptMask, Gain::A128, 5)
            .unwrap_err();
        assert_eq!(err, Error::NotReady);
        assert_eq!(delay.polls, 5);
        assert_eq!(bus.edges.get(), 0);
    }

    #[test]
    fn read_blocking_zero_polls_reads_when_ready() {
        let (mut adc, _bus) = driver(0x12_3456);
        let mut delay = CountingDelay::default();
        let raw = adc
            .read_blocking(&mut delay, &NoopInterruptMask, Gain::A128, 0)
            .unwrap();
        assert_eq!(raw, 0x12_3456);
        assert_eq!(delay.polls, 0);
    }

    #[test]
    fn read_blocking_zero_polls_is_one_non_blocking_attempt() {
        let (mut adc, bus) = driver(0x12_3456);
        bus.busy_polls.set(u32::MAX);
        let mut delay = CountingDelay::default();
        let err = adc
            .read_blocking(&mut delay, &NoopInterruptMask, Gain::A128, 0)
            .unwrap_err();
        assert_eq!(err, Error::NotReady);
        assert_eq!(delay.polls, 0);
        assert_eq!(bus.edges.get(), 0);
    }

    #[test]
    fn read_blocking_reads_once_dout_drops() {
        let (mut adc, bus) = driver(0x12_3456);
        bus.busy_polls.set(3);
        let mut delay = CountingDelay::default();
        let raw = adc
            .read_blocking(&mut delay, &NoopInterruptMask, Gain::A128, 10)
            .unwrap();
        assert_eq!(raw, 0x12_3456);
        assert_eq!(delay.polls, 3);
    }

    #[test]
    fn display_names_each_variant() {
        extern crate alloc;
        use alloc::format;
        let e: Error<&str, &str> = Error::NotReady;
        assert!(format!("{e}").contains("not ready"));
        let e: Error<&str, &str> = Error::Dout("x");
        assert!(format!("{e}").contains("DOUT"));
        let e: Error<&str, &str> = Error::Sck("x");
        assert!(format!("{e}").contains("PD_SCK"));
    }
}
