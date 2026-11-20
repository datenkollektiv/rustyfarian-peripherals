//! Sensirion SHT3x (SHT30 / SHT31 / SHT35) protocol constants, CRC check, and fixed-point conversion.
//!
//! Provides platform-independent types and functions for the SHT3x
//! temperature and humidity sensor over I2C, in single-shot mode without
//! clock stretching. The pure part interprets the datasheet's command set,
//! CRC-8 and byte layout: it imports no I2C, HAL, or chip crate and uses no
//! floating point. Results are fixed point: [`Measurement::temperature_centi_c`](crate::sht3x::Measurement::temperature_centi_c)
//! in hundredths of a degree Celsius and [`Measurement::humidity_centi_rh`](crate::sht3x::Measurement::humidity_centi_rh)
//! in hundredths of a percent, both rounded to nearest.
//!
//! With the `hal` feature, `Sht3x` adds a blocking
//! driver generic over `embedded_hal::i2c::I2c` and
//! `embedded_hal::delay::DelayNs` (see ADR-009), so the same code runs on
//! esp-hal and esp-idf-hal. With `hal-async`, `Sht3x::measure_async` runs the
//! same sequence over `embedded-hal-async`. Both runners are thin loops over
//! the sans-io [`SingleShot`](crate::sht3x::SingleShot) action machine (ADR-010), the single source of
//! the command / wait / read order.
//!
//! This module is device-specific by design: the command words, timings and
//! CRC are fixed SHT3x-DIS datasheet facts (v7), not a generic
//! temperature-sensor abstraction. Periodic and ART modes, alert thresholds
//! and the SHT4x are out of scope.
//!
//! # Protocol
//!
//! The sensor listens on [`I2C_ADDR`](crate::sht3x::I2C_ADDR) (`0x44`) by default, or [`I2C_ADDR_ALT`](crate::sht3x::I2C_ADDR_ALT)
//! (`0x45`) when the ADDR pin is pulled high.
//! A measurement is one command ([`Repeatability::command`]), a wait of
//! [`Repeatability::max_duration_us`], then a single 6-byte read
//! `[T_msb, T_lsb, T_crc, RH_msb, RH_lsb, RH_crc]` that
//! [`parse_measurement`](crate::sht3x::parse_measurement) checks and converts.
//! Without clock stretching a sensor that is not ready answers the read
//! header with a NACK instead of holding SCL.
//! The datasheet asks for at least 1 ms between two commands.
//!
//! # Example
//!
//! ```
//! use tamer::sht3x::{crc8, parse_measurement};
//!
//! // A frame as the sensor sends it: 25.00 degC, 50.00 %RH.
//! let t = [0x66, 0x66];
//! let rh = [0x80, 0x00];
//! let frame = [t[0], t[1], crc8(t), rh[0], rh[1], crc8(rh)];
//!
//! let m = parse_measurement(&frame).unwrap();
//! assert_eq!(m.temperature_centi_c(), 2_500);
//! assert_eq!(m.humidity_centi_rh(), 5_000);
//! ```

#[cfg(feature = "hal")]
use embedded_hal::delay::DelayNs;
#[cfg(feature = "hal")]
use embedded_hal::i2c::{ErrorKind, I2c};

// ─── Protocol constants ─────────────────────────────────────────────────────

/// Default I2C address (ADDR pin low).
pub const I2C_ADDR: u8 = 0x44;

/// Alternate I2C address (ADDR pin high).
pub const I2C_ADDR_ALT: u8 = 0x45;

/// Soft-reset command word (datasheet Table 9).
pub const CMD_SOFT_RESET: [u8; 2] = [0x30, 0xA2];

/// Heater-on command word. Shipped as data; the driver has no heater method in v1.
pub const CMD_HEATER_ON: [u8; 2] = [0x30, 0x6D];

/// Heater-off command word. Shipped as data; the driver has no heater method in v1.
pub const CMD_HEATER_OFF: [u8; 2] = [0x30, 0x66];

/// Read-status-register command word (datasheet Table 17).
pub const CMD_READ_STATUS: [u8; 2] = [0xF3, 0x2D];

/// Clear-status-register command word (datasheet Table 19).
/// Shipped as data; the driver has no clear-status method in v1.
pub const CMD_CLEAR_STATUS: [u8; 2] = [0x30, 0x41];

/// Power-up wait before the first command, in microseconds.
///
/// Covers `t_PU` = 1.5 ms (datasheet Table 5; Table 4 says 1 ms).
#[cfg(feature = "hal")]
const POWER_UP_US: u32 = 2_000;

/// Wait after the soft-reset command, in microseconds.
///
/// Covers `t_SR` = 1.5 ms (datasheet Table 4; Table 5 lists no `t_SR`),
/// counted from the ACK of the reset command.
#[cfg(feature = "hal")]
const SOFT_RESET_US: u32 = 2_000;

/// Minimum spacing between two commands, in microseconds (datasheet section 4.1).
#[cfg(feature = "hal")]
const COMMAND_GAP_US: u32 = 1_000;

/// Divisor of the raw-to-physical scaling: the full 16-bit raw range.
const RAW_FULL: u32 = 65_535;

// ─── Repeatability ──────────────────────────────────────────────────────────

/// Single-shot measurement repeatability, clock stretching disabled.
///
/// Higher repeatability lowers noise and lengthens the measurement.
/// The consumer default is [`High`](Self::High).
/// The enum is exhaustive; adding a variant is a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeatability {
    /// High repeatability: command `0x2400`, up to 15.5 ms.
    High,
    /// Medium repeatability: command `0x240B`, up to 6.5 ms.
    Medium,
    /// Low repeatability: command `0x2416`, up to 4.5 ms.
    Low,
}

impl Repeatability {
    /// Command word for a single-shot measurement without clock stretching
    /// (datasheet Table 9), most significant byte first.
    #[must_use]
    pub const fn command(self) -> [u8; 2] {
        match self {
            Self::High => [0x24, 0x00],
            Self::Medium => [0x24, 0x0B],
            Self::Low => [0x24, 0x16],
        }
    }

    /// Maximum measurement duration in microseconds.
    ///
    /// These are the datasheet Table 5 maxima for a 2.15 V to below 2.4 V
    /// supply (15.5 / 6.5 / 4.5 ms), which also cover the shorter Table 4
    /// maxima (15 / 6 / 4 ms) for supplies of 2.4 V and up.
    #[must_use]
    pub const fn max_duration_us(self) -> u32 {
        match self {
            Self::High => 15_500,
            Self::Medium => 6_500,
            Self::Low => 4_500,
        }
    }
}

// ─── CRC and conversion ─────────────────────────────────────────────────────

/// CRC-8 over one 2-byte word: polynomial `0x31`, init `0xFF`, no reflection,
/// no final XOR (datasheet Table 20).
///
/// ```
/// // Datasheet example: CRC(0xBEEF) = 0x92.
/// assert_eq!(tamer::sht3x::crc8([0xBE, 0xEF]), 0x92);
/// ```
#[must_use]
pub const fn crc8(word: [u8; 2]) -> u8 {
    let mut crc: u8 = 0xFF;
    let mut i = 0;
    while i < 2 {
        crc ^= word[i];
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x31
            } else {
                crc << 1
            };
            bit += 1;
        }
        i += 1;
    }
    crc
}

/// Converts a raw temperature word to hundredths of a degree Celsius.
///
/// `T = -45 + 175 * raw / 65535`, rounded to nearest (`-4500` to `13000`).
/// The widest intermediate is `17_500 * 65_535 + 32_767`, which fits in `i32`.
#[must_use]
pub const fn raw_to_centi_celsius(raw: u16) -> i32 {
    -4_500 + (17_500 * raw as i32 + (RAW_FULL / 2) as i32) / RAW_FULL as i32
}

/// Converts a raw humidity word to hundredths of a percent relative humidity.
///
/// `RH = 100 * raw / 65535`, rounded to nearest (`0` to `10000`).
/// The widest intermediate is `10_000 * 65_535 + 32_767`, which fits in `u32`.
#[must_use]
pub const fn raw_to_centi_percent_rh(raw: u16) -> u16 {
    // At most 10_000 by construction, so the narrowing cast cannot truncate.
    ((10_000 * raw as u32 + RAW_FULL / 2) / RAW_FULL) as u16
}

// ─── Parsed values ──────────────────────────────────────────────────────────

/// The failing word of a response whose CRC did not match.
///
/// The enum is exhaustive; adding a variant is a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrcError {
    /// The temperature word failed its CRC.
    Temperature,
    /// The humidity word failed its CRC.
    Humidity,
    /// The status word failed its CRC.
    Status,
}

impl core::fmt::Display for CrcError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let word = match self {
            Self::Temperature => "temperature",
            Self::Humidity => "humidity",
            Self::Status => "status",
        };
        write!(f, "CRC mismatch in the {word} word")
    }
}

/// One measurement in fixed point.
///
/// Built only by [`parse_measurement`]. The fields are private so the layout
/// can grow without a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measurement {
    raw_temperature: u16,
    raw_humidity: u16,
}

impl Measurement {
    /// Temperature in hundredths of a degree Celsius (`2512` is 25.12 degC).
    #[must_use]
    pub const fn temperature_centi_c(&self) -> i32 {
        raw_to_centi_celsius(self.raw_temperature)
    }

    /// Relative humidity in hundredths of a percent (`4875` is 48.75 %RH).
    #[must_use]
    pub const fn humidity_centi_rh(&self) -> u16 {
        raw_to_centi_percent_rh(self.raw_humidity)
    }

    /// The raw 16-bit temperature word as sent by the sensor.
    #[must_use]
    pub const fn raw_temperature(&self) -> u16 {
        self.raw_temperature
    }

    /// The raw 16-bit humidity word as sent by the sensor.
    #[must_use]
    pub const fn raw_humidity(&self) -> u16 {
        self.raw_humidity
    }
}

/// The 16-bit status register (datasheet Table 18).
///
/// Built only by [`parse_status`]. Reserved bits are kept in
/// [`bits`](Self::bits) but have no accessor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    bits: u16,
}

impl Status {
    const ALERT_PENDING: u16 = 1 << 15;
    const HEATER_ON: u16 = 1 << 13;
    const RESET_DETECTED: u16 = 1 << 4;
    const COMMAND_FAILED: u16 = 1 << 1;
    const CHECKSUM_FAILED: u16 = 1;

    /// The raw status word.
    ///
    /// After a reset it is `0x8010` only when the reserved bits 9:5 read
    /// zero (the datasheet shows them as `xxxxx`), so never compare the
    /// whole word to decide whether the sensor is present.
    #[must_use]
    pub const fn bits(&self) -> u16 {
        self.bits
    }

    /// Bit 4: a reset was detected since the last clear-status command.
    ///
    /// Set after power-up, a hard reset, a soft reset or a supply failure.
    #[must_use]
    pub const fn reset_detected(&self) -> bool {
        self.bits & Self::RESET_DETECTED != 0
    }

    /// Bit 13: the heater is on.
    #[must_use]
    pub const fn heater_on(&self) -> bool {
        self.bits & Self::HEATER_ON != 0
    }

    /// Bit 15: at least one alert is pending.
    ///
    /// The datasheet default after a reset is `1`, so this is true on a
    /// fresh sensor and is not a fault.
    #[must_use]
    pub const fn alert_pending(&self) -> bool {
        self.bits & Self::ALERT_PENDING != 0
    }

    /// Bit 1: the last command was not processed (invalid, or it failed its
    /// integrated checksum).
    #[must_use]
    pub const fn command_failed(&self) -> bool {
        self.bits & Self::COMMAND_FAILED != 0
    }

    /// Bit 0: the checksum of the last write transfer failed.
    #[must_use]
    pub const fn checksum_failed(&self) -> bool {
        self.bits & Self::CHECKSUM_FAILED != 0
    }
}

/// Parses the 6-byte response `[T_msb, T_lsb, T_crc, RH_msb, RH_lsb, RH_crc]`.
///
/// The temperature word is checked first, so a frame with both CRCs wrong
/// reports [`CrcError::Temperature`].
///
/// # Errors
///
/// Returns [`CrcError::Temperature`] or [`CrcError::Humidity`] naming the
/// word whose CRC does not match.
pub const fn parse_measurement(frame: &[u8; 6]) -> Result<Measurement, CrcError> {
    let t = [frame[0], frame[1]];
    let rh = [frame[3], frame[4]];
    if crc8(t) != frame[2] {
        return Err(CrcError::Temperature);
    }
    if crc8(rh) != frame[5] {
        return Err(CrcError::Humidity);
    }
    Ok(Measurement {
        raw_temperature: u16::from_be_bytes(t),
        raw_humidity: u16::from_be_bytes(rh),
    })
}

/// Parses the 3-byte status response `[S_msb, S_lsb, S_crc]`.
///
/// # Errors
///
/// Returns [`CrcError::Status`] if the CRC does not match.
pub const fn parse_status(frame: &[u8; 3]) -> Result<Status, CrcError> {
    let word = [frame[0], frame[1]];
    if crc8(word) != frame[2] {
        return Err(CrcError::Status);
    }
    Ok(Status {
        bits: u16::from_be_bytes(word),
    })
}

// ─── SingleShot (rung-3 action machine) ─────────────────────────────────────

/// Input fed to [`SingleShot::poll`]: the answer to the previous action.
///
/// The enum is exhaustive; adding a variant is a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotInput {
    /// Begin the run. Valid only on a fresh machine.
    Start,
    /// The command write requested by [`ShotAction::WriteCommand`] was acknowledged.
    Written,
    /// The pause requested by [`ShotAction::Wait`] has elapsed.
    Waited,
    /// The 6 bytes answering [`ShotAction::ReadFrame`].
    Frame([u8; 6]),
}

/// Action requested by [`SingleShot::poll`].
///
/// The enum is exhaustive; adding a variant is a breaking change.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShotAction {
    /// `i2c.write(addr, &command)`, answer [`ShotInput::Written`].
    WriteCommand([u8; 2]),
    /// `delay.delay_us(us)`, answer [`ShotInput::Waited`].
    Wait {
        /// Pause length in microseconds.
        us: u32,
    },
    /// `i2c.read(addr, &mut [u8; 6])`, answer [`ShotInput::Frame`].
    ReadFrame,
    /// Terminal: the parsed frame.
    Done(Measurement),
    /// Terminal: the word whose CRC failed.
    Fail(CrcError),
    /// No-op: the input did not fit the machine's phase.
    /// Runners treat this as an error exit.
    OutOfPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShotState {
    /// A command from an earlier run may still be converting: wait it out.
    Recovering,
    /// The recovery wait has elapsed; the command may go out.
    Settling,
    Idle,
    Writing,
    Waiting,
    Reading,
    Finished,
}

/// Sans-io action machine behind `Sht3x::measure` and `Sht3x::measure_async`.
///
/// It requests the measurement command, one wait of
/// [`Repeatability::max_duration_us`], and one 6-byte read, then finishes
/// with [`ShotAction::Done`] or [`ShotAction::Fail`]. It never reads a clock and never touches a
/// bus: a runner performs each action and feeds the answer back.
/// [`recovering`](Self::recovering) prepends one wait for a command that an
/// earlier, abandoned run may have left converting.
///
/// ```
/// use tamer::sht3x::{crc8, Repeatability, ShotAction, ShotInput, SingleShot};
///
/// let mut m = SingleShot::new(Repeatability::High);
/// assert_eq!(m.poll(ShotInput::Start), ShotAction::WriteCommand([0x24, 0x00]));
/// assert_eq!(m.poll(ShotInput::Written), ShotAction::Wait { us: 15_500 });
/// assert_eq!(m.poll(ShotInput::Waited), ShotAction::ReadFrame);
///
/// let t = [0x66, 0x66];
/// let rh = [0x80, 0x00];
/// let frame = [t[0], t[1], crc8(t), rh[0], rh[1], crc8(rh)];
/// let ShotAction::Done(reading) = m.poll(ShotInput::Frame(frame)) else { panic!() };
/// assert_eq!(reading.temperature_centi_c(), 2_500);
/// assert!(m.is_finished());
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SingleShot {
    state: ShotState,
    repeatability: Repeatability,
    /// Leading wait for a command still in flight; 0 on a fresh run.
    recovery_us: u32,
}

impl SingleShot {
    /// Creates a machine for one measurement at `repeatability`. Does no work.
    #[must_use]
    pub const fn new(repeatability: Repeatability) -> Self {
        Self {
            state: ShotState::Idle,
            repeatability,
            recovery_us: 0,
        }
    }

    /// Creates a machine that first waits out a command left unanswered;
    /// `in_flight` is that command's repeatability.
    ///
    /// After a cancelled run, or a read that found the sensor not ready, the
    /// sensor may still be converting. The first action is then
    /// `Wait { us: in_flight.max_duration_us() }`, so the new command meets
    /// an idle sensor and keeps the datasheet's 1 ms command spacing
    /// (section 4.4 asks for a break command before a command during a
    /// measurement; the wait needs no extra command).
    #[must_use]
    pub const fn recovering(repeatability: Repeatability, in_flight: Repeatability) -> Self {
        Self {
            state: ShotState::Recovering,
            repeatability,
            recovery_us: in_flight.max_duration_us(),
        }
    }

    /// Advances the machine by one input and returns the next action.
    ///
    /// An input that does not fit the current phase yields
    /// [`ShotAction::OutOfPhase`] and leaves the machine unchanged.
    /// After a terminal action every input yields `OutOfPhase`.
    pub fn poll(&mut self, input: ShotInput) -> ShotAction {
        match (self.state, input) {
            (ShotState::Recovering, ShotInput::Start) => {
                self.state = ShotState::Settling;
                ShotAction::Wait {
                    us: self.recovery_us,
                }
            }
            (ShotState::Settling, ShotInput::Waited) | (ShotState::Idle, ShotInput::Start) => {
                self.state = ShotState::Writing;
                ShotAction::WriteCommand(self.repeatability.command())
            }
            (ShotState::Writing, ShotInput::Written) => {
                self.state = ShotState::Waiting;
                ShotAction::Wait {
                    us: self.repeatability.max_duration_us(),
                }
            }
            (ShotState::Waiting, ShotInput::Waited) => {
                self.state = ShotState::Reading;
                ShotAction::ReadFrame
            }
            (ShotState::Reading, ShotInput::Frame(frame)) => {
                self.state = ShotState::Finished;
                match parse_measurement(&frame) {
                    Ok(m) => ShotAction::Done(m),
                    Err(e) => ShotAction::Fail(e),
                }
            }
            _ => ShotAction::OutOfPhase,
        }
    }

    /// Returns `true` once a terminal action has been emitted.
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        matches!(self.state, ShotState::Finished)
    }

    /// Upper bound on the polls a complete run needs, for runner fuel.
    ///
    /// A run takes four polls (start, written, waited, frame), five after
    /// [`recovering`](Self::recovering); the bound adds slack, as
    /// `Mpu6050`'s sampler does.
    #[must_use]
    pub const fn max_polls(&self) -> u64 {
        8
    }
}

// ─── Error ───────────────────────────────────────────────────────────────────

/// Error returned by the [`Sht3x`] driver.
///
/// `E` is the HAL's own I2C error type, kept intact in [`Bus`](Self::Bus) so
/// callers can inspect it. There is deliberately no `From<E>` impl: map bus
/// errors explicitly with `Error::Bus` to keep `?` conversions unambiguous.
#[cfg(feature = "hal")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error<E> {
    /// The I2C bus reported an error.
    Bus(E),
    /// A response failed its CRC check; the payload names the word.
    Crc(CrcError),
    /// The driver and its state machine fell out of step.
    ///
    /// Returned by `measure` and `measure_async` when the machine answers
    /// [`ShotAction::OutOfPhase`] or the poll fuel runs out.
    /// This is a library bug, never a device or bus condition.
    Sequence,
}

#[cfg(feature = "hal")]
impl<E> From<CrcError> for Error<E> {
    fn from(e: CrcError) -> Self {
        Self::Crc(e)
    }
}

#[cfg(feature = "hal")]
impl<E: embedded_hal::i2c::Error> Error<E> {
    /// Returns `true` if this is a bus error of kind
    /// [`ErrorKind::NoAcknowledge`].
    ///
    /// This is the portable way to tell "no device answered" from a genuine
    /// bus fault: esp-hal maps `AcknowledgeCheckFailed` and esp-idf-hal maps
    /// `ESP_FAIL` to `NoAcknowledge`.
    /// On the measurement read it also means "not ready yet", because a
    /// busy SHT3x NACKs the read header.
    #[must_use]
    pub fn is_nack(&self) -> bool {
        match self {
            Self::Bus(e) => matches!(e.kind(), ErrorKind::NoAcknowledge(_)),
            Self::Crc(_) | Self::Sequence => false,
        }
    }
}

#[cfg(feature = "hal")]
impl<E: core::fmt::Debug> core::fmt::Display for Error<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Bus(e) => write!(f, "I2C bus error: {e:?}"),
            Self::Crc(e) => write!(f, "SHT3x response corrupted: {e}"),
            Self::Sequence => write!(
                f,
                "the driver and its state machine fell out of step; \
                 this is a library bug - please report"
            ),
        }
    }
}

// ─── Sht3x driver ────────────────────────────────────────────────────────────

/// Blocking SHT3x driver over an `embedded_hal` I2C bus.
///
/// The driver owns the bus; hand it back with [`release`](Self::release). All
/// protocol knowledge comes from this module's constants and parsers, so the
/// esp-hal and esp-idf tiers share it unchanged.
///
/// Every command is a plain write and every response a separate plain read,
/// each ending with a STOP as the datasheet requires.
/// No two commands are issued closer than the datasheet's 1 ms, in any call order.
///
/// # Timing
///
/// The waits must not be shorter than requested.
/// Use a busy-wait or a 1 kHz tick delay: a coarse RTOS tick that rounds a
/// wait down makes the single read NACK as "not ready".
///
/// ```
/// use tamer::mock::{MockI2c, NoopDelay, Transaction};
/// use tamer::sht3x::{self, Repeatability, Sht3x};
///
/// let status = [0x80, 0x10, sht3x::crc8([0x80, 0x10])];
/// let t = [0x66, 0x66];
/// let rh = [0x80, 0x00];
/// let frame = [t[0], t[1], sht3x::crc8(t), rh[0], rh[1], sht3x::crc8(rh)];
/// let script = [
///     Transaction::Write { address: sht3x::I2C_ADDR, bytes: &sht3x::CMD_SOFT_RESET },
///     Transaction::Write { address: sht3x::I2C_ADDR, bytes: &sht3x::CMD_READ_STATUS },
///     Transaction::Read { address: sht3x::I2C_ADDR, response: &status },
///     Transaction::Write { address: sht3x::I2C_ADDR, bytes: &[0x24, 0x00] },
///     Transaction::Read { address: sht3x::I2C_ADDR, response: &frame },
/// ];
/// let mut sensor = Sht3x::new(MockI2c::new(&script), sht3x::I2C_ADDR);
/// let mut delay = NoopDelay;
///
/// let st = sensor.init(&mut delay).unwrap();
/// assert!(st.reset_detected());
///
/// let m = sensor.measure(&mut delay, Repeatability::High).unwrap();
/// assert_eq!(m.temperature_centi_c(), 2_500);
/// assert_eq!(m.humidity_centi_rh(), 5_000);
/// sensor.release().done();
/// ```
#[cfg(feature = "hal")]
#[derive(Debug)]
pub struct Sht3x<I2C> {
    i2c: I2C,
    address: u8,
    /// Repeatability of a measurement command that may still be converting:
    /// set before the command goes out, cleared once a frame is read or the
    /// write is refused.
    in_flight: Option<Repeatability>,
}

/// Construction and release need no bus trait, so a bus that implements only
/// the `embedded-hal-async` `I2c` (an embassy shared-bus device, say) can hold
/// the driver and use `measure_async`.
#[cfg(feature = "hal")]
impl<I2C> Sht3x<I2C> {
    /// Creates a driver on `i2c` for the device at `address`.
    ///
    /// Pass [`I2C_ADDR`] or [`I2C_ADDR_ALT`] (ADDR high). No bus traffic
    /// occurs until [`init`](Self::init), so a consumer that powers the
    /// sensor from a switched rail can construct the driver first.
    #[must_use]
    pub const fn new(i2c: I2C, address: u8) -> Self {
        Self {
            i2c,
            address,
            in_flight: None,
        }
    }

    /// Consumes the driver and returns the I2C bus.
    #[must_use]
    pub fn release(self) -> I2C {
        self.i2c
    }

    /// Builds the measurement machine, waiting out a command in flight first.
    const fn shot(&self, repeatability: Repeatability) -> SingleShot {
        match self.in_flight {
            Some(pending) => SingleShot::recovering(repeatability, pending),
            None => SingleShot::new(repeatability),
        }
    }
}

#[cfg(feature = "hal")]
impl<I2C: I2c> Sht3x<I2C> {
    fn write_command(&mut self, command: [u8; 2]) -> Result<(), Error<I2C::Error>> {
        self.i2c.write(self.address, &command).map_err(Error::Bus)
    }

    /// Waits out a measurement command still in flight, if any, so the next
    /// command meets an idle sensor and keeps the 1 ms spacing.
    fn settle<D: DelayNs>(&mut self, delay: &mut D) {
        if let Some(pending) = self.in_flight.take() {
            delay.delay_us(pending.max_duration_us());
        }
    }

    /// Writes `command`, then waits `gap_us` so the next command stays that
    /// far behind it. A write that fails with anything but a NACK may still
    /// have reached the sensor, so the wait happens on that path too; a
    /// refused write (NACK) needs none.
    fn write_command_then_wait<D: DelayNs>(
        &mut self,
        command: [u8; 2],
        gap_us: u32,
        delay: &mut D,
    ) -> Result<(), Error<I2C::Error>> {
        let result = self.write_command(command);
        let refused = matches!(&result, Err(e) if e.is_nack());
        if !refused {
            delay.delay_us(gap_us);
        }
        result
    }

    /// Brings the sensor up and returns its status register.
    ///
    /// Waits 2 ms for power-up, soft-resets, then reads the CRC-checked
    /// status, 5 ms in total. The status read is the presence check; a
    /// missing device surfaces as a NACK, see [`Error::is_nack`].
    /// The reset-detected bit is not asserted: presence is enough, and a
    /// stale bit is not a fault.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Bus`] if any transfer fails, or [`Error::Crc`] with
    /// [`CrcError::Status`] if the status word is corrupted.
    pub fn init<D: DelayNs>(&mut self, delay: &mut D) -> Result<Status, Error<I2C::Error>> {
        delay.delay_us(POWER_UP_US);
        self.soft_reset(delay)?;
        self.status(delay)
    }

    /// Sends the soft-reset command and waits 2 ms for it to complete.
    ///
    /// The wait covers `t_SR`, counted from the ACK of the command, and
    /// leaves the bus quiet for the datasheet's 1 ms command spacing.
    /// A measurement command still in flight (a cancelled `measure_async`,
    /// or a read that found the sensor not ready) is waited out first.
    /// A write that fails with anything but a NACK may have reached the
    /// sensor, so the wait still happens before the error returns; a refused
    /// write returns at once.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Bus`] if the write fails.
    pub fn soft_reset<D: DelayNs>(&mut self, delay: &mut D) -> Result<(), Error<I2C::Error>> {
        self.settle(delay);
        self.write_command_then_wait(CMD_SOFT_RESET, SOFT_RESET_US, delay)
    }

    /// Reads the status register.
    ///
    /// Writes [`CMD_READ_STATUS`], waits 1 ms, then reads 3 bytes. The wait
    /// keeps the next command at least 1 ms behind this one, so any call
    /// may follow. A measurement command still in flight is waited out
    /// first, and a write that fails with anything but a NACK still waits
    /// the 1 ms before the error returns.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Bus`] if a transfer fails, or [`Error::Crc`] with
    /// [`CrcError::Status`] if the CRC does not match.
    pub fn status<D: DelayNs>(&mut self, delay: &mut D) -> Result<Status, Error<I2C::Error>> {
        self.settle(delay);
        self.write_command_then_wait(CMD_READ_STATUS, COMMAND_GAP_US, delay)?;
        let mut buf = [0u8; 3];
        self.i2c.read(self.address, &mut buf).map_err(Error::Bus)?;
        Ok(parse_status(&buf)?)
    }

    /// Takes one single-shot measurement.
    ///
    /// A thin blocking runner over [`SingleShot`], the single source of the
    /// command / wait / read sequence.
    /// It sends the command, waits exactly
    /// [`max_duration_us`](Repeatability::max_duration_us), then reads 6
    /// bytes once. A NACK on that read is reported, not retried.
    /// If a command is still in flight (a read that found the sensor not
    /// ready, or a cancelled `measure_async`), the run first waits that
    /// repeatability's maximum duration, see [`SingleShot::recovering`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::Bus`] if a transfer fails; a NACK on the read
    /// ([`Error::is_nack`]) means the sensor is absent or not ready. After a
    /// not-ready read the command stays in flight for the next call to wait
    /// out; a refused write clears it, since no conversion started.
    /// Returns [`Error::Crc`] naming the first word (temperature, then
    /// humidity) whose CRC does not match.
    /// Returns [`Error::Sequence`] if the state machine answers
    /// [`ShotAction::OutOfPhase`] or the poll fuel runs out (a library bug).
    pub fn measure<D: DelayNs>(
        &mut self,
        delay: &mut D,
        repeatability: Repeatability,
    ) -> Result<Measurement, Error<I2C::Error>> {
        let mut shot = self.shot(repeatability);
        let mut input = ShotInput::Start;
        for _ in 0..shot.max_polls() {
            input = match shot.poll(input) {
                ShotAction::WriteCommand(command) => {
                    // Set before the write: a command that may have landed
                    // counts as in flight until a frame comes back. A refused
                    // write (NACK) never started a conversion.
                    self.in_flight = Some(repeatability);
                    if let Err(e) = self.write_command(command) {
                        if e.is_nack() {
                            self.in_flight = None;
                        }
                        return Err(e);
                    }
                    ShotInput::Written
                }
                ShotAction::Wait { us } => {
                    delay.delay_us(us);
                    ShotInput::Waited
                }
                ShotAction::ReadFrame => {
                    let mut frame = [0u8; 6];
                    self.i2c
                        .read(self.address, &mut frame)
                        .map_err(Error::Bus)?;
                    self.in_flight = None;
                    ShotInput::Frame(frame)
                }
                ShotAction::Done(m) => return Ok(m),
                ShotAction::Fail(e) => return Err(e.into()),
                ShotAction::OutOfPhase => break,
            };
        }
        // Unreachable: each input is built from the action just received.
        Err(Error::Sequence)
    }
}

/// Async runner (feature `hal-async`), for executors such as embassy.
#[cfg(feature = "hal-async")]
impl<I2C: embedded_hal_async::i2c::I2c> Sht3x<I2C> {
    /// Async twin of [`measure`](Self::measure): the same sequence over
    /// `embedded-hal-async` `I2c` and `DelayNs`.
    ///
    /// It drives the same [`SingleShot`] with the same error mapping,
    /// including [`Error::Sequence`] and the poll-fuel bound.
    /// Bring-up ([`init`](Self::init), [`soft_reset`](Self::soft_reset),
    /// [`status`](Self::status)) is blocking only, as for the other
    /// drivers; initialise before moving to an async-only bus.
    ///
    /// # Cancellation
    ///
    /// Dropping the future mid-measurement leaves the sensor converting, and
    /// the driver remembers the command as in flight. The next call on this
    /// driver (`measure`, `measure_async`, `soft_reset` or `status`) first
    /// waits that repeatability's maximum duration, so no command lands on a
    /// busy sensor or inside the datasheet's 1 ms command spacing; see
    /// [`SingleShot::recovering`].
    ///
    /// # Errors
    ///
    /// The same as [`measure`](Self::measure).
    pub async fn measure_async<D: embedded_hal_async::delay::DelayNs>(
        &mut self,
        delay: &mut D,
        repeatability: Repeatability,
    ) -> Result<Measurement, Error<I2C::Error>> {
        use embedded_hal_async::i2c::I2c as AsyncI2c;

        let mut shot = self.shot(repeatability);
        let mut input = ShotInput::Start;
        for _ in 0..shot.max_polls() {
            input = match shot.poll(input) {
                ShotAction::WriteCommand(command) => {
                    // As in `measure`: in flight from before the write until a
                    // frame, unless the write itself is refused.
                    self.in_flight = Some(repeatability);
                    if let Err(e) = AsyncI2c::write(&mut self.i2c, self.address, &command).await {
                        let e = Error::Bus(e);
                        if e.is_nack() {
                            self.in_flight = None;
                        }
                        return Err(e);
                    }
                    ShotInput::Written
                }
                ShotAction::Wait { us } => {
                    embedded_hal_async::delay::DelayNs::delay_us(delay, us).await;
                    ShotInput::Waited
                }
                ShotAction::ReadFrame => {
                    let mut frame = [0u8; 6];
                    AsyncI2c::read(&mut self.i2c, self.address, &mut frame)
                        .await
                        .map_err(Error::Bus)?;
                    self.in_flight = None;
                    ShotInput::Frame(frame)
                }
                ShotAction::Done(m) => return Ok(m),
                ShotAction::Fail(e) => return Err(e.into()),
                ShotAction::OutOfPhase => break,
            };
        }
        // Unreachable, as in the blocking runner.
        Err(Error::Sequence)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(t: [u8; 2], rh: [u8; 2]) -> [u8; 6] {
        [t[0], t[1], crc8(t), rh[0], rh[1], crc8(rh)]
    }

    #[test]
    fn crc_matches_datasheet_example() {
        // SHT3x-DIS datasheet Table 20: CRC(0xBEEF) = 0x92.
        assert_eq!(crc8([0xBE, 0xEF]), 0x92);
    }

    #[test]
    fn crc_fixtures() {
        assert_eq!(crc8([0x80, 0x10]), 0xE1); // default status
        assert_eq!(crc8([0x66, 0x66]), 0x93);
        assert_eq!(crc8([0x80, 0x00]), 0xA2);
        assert_eq!(crc8([0x74, 0x4B]), 0xEE);
    }

    #[test]
    fn temperature_range_endpoints() {
        assert_eq!(raw_to_centi_celsius(0), -4_500);
        assert_eq!(raw_to_centi_celsius(u16::MAX), 13_000);
    }

    #[test]
    fn humidity_range_endpoints() {
        assert_eq!(raw_to_centi_percent_rh(0), 0);
        assert_eq!(raw_to_centi_percent_rh(u16::MAX), 10_000);
    }

    #[test]
    fn temperature_0x6666_is_25_00() {
        assert_eq!(raw_to_centi_celsius(0x6666), 2_500);
    }

    #[test]
    fn humidity_0x8000_is_50_00() {
        assert_eq!(raw_to_centi_percent_rh(0x8000), 5_000);
    }

    #[test]
    fn temperature_rounds_to_nearest_at_half_step() {
        // 17_500 * raw mod 65_535 is a multiple of 5, so no raw sits exactly
        // on a tie; these are the closest ones on each side.
        // Residue 32_765 is just below half a step: rounds down.
        assert_eq!(raw_to_centi_celsius(455), -4_379);
        // Residue 32_770 is just above half a step: rounds up (truncation
        // would give -1_122).
        assert_eq!(raw_to_centi_celsius(12_652), -1_121);
        // Truncation would give -4_500.
        assert_eq!(raw_to_centi_celsius(2), -4_499);
    }

    #[test]
    fn humidity_rounds_to_nearest_at_half_step() {
        assert_eq!(raw_to_centi_percent_rh(4_073), 621);
        // Truncation would give 1_378.
        assert_eq!(raw_to_centi_percent_rh(9_034), 1_379);
        // Truncation would give 0.
        assert_eq!(raw_to_centi_percent_rh(4), 1);
    }

    #[test]
    fn conversions_match_exact_rounding_for_every_raw() {
        // Reference: floor((2n + d) / (2d)) in u64, the exact round-half-up.
        let d = u64::from(RAW_FULL);
        for raw in 0..=u16::MAX {
            let n = u64::from(raw);
            let t = (2 * 17_500 * n + d) / (2 * d);
            assert_eq!(i64::from(raw_to_centi_celsius(raw)), t as i64 - 4_500);
            let rh = (2 * 10_000 * n + d) / (2 * d);
            assert_eq!(u64::from(raw_to_centi_percent_rh(raw)), rh);
        }
    }

    #[test]
    fn brood_range_resolution() {
        // 34.5 degC is raw 0x744B; one raw step is well under 0.01 degC.
        assert_eq!(raw_to_centi_celsius(0x744B), 3_450);
        assert_eq!(raw_to_centi_celsius(0x744C), 3_450);
        for raw in 0..u16::MAX {
            let step = raw_to_centi_celsius(raw + 1) - raw_to_centi_celsius(raw);
            assert!((0..=1).contains(&step), "raw {raw}: step {step}");
        }
    }

    #[test]
    fn measurement_accessors_return_raw_and_scaled() {
        let m = parse_measurement(&frame([0x66, 0x66], [0x80, 0x00])).unwrap();
        assert_eq!(m.raw_temperature(), 0x6666);
        assert_eq!(m.raw_humidity(), 0x8000);
        assert_eq!(m.temperature_centi_c(), 2_500);
        assert_eq!(m.humidity_centi_rh(), 5_000);
    }

    #[test]
    fn parse_measurement_valid_frame() {
        let m = parse_measurement(&frame([0x74, 0x4B], [0x80, 0x00])).unwrap();
        assert_eq!(m.temperature_centi_c(), 3_450);
        assert_eq!(m.humidity_centi_rh(), 5_000);
    }

    #[test]
    fn parse_measurement_bad_temperature_crc() {
        let mut f = frame([0x66, 0x66], [0x80, 0x00]);
        f[2] ^= 0x01;
        assert_eq!(parse_measurement(&f), Err(CrcError::Temperature));
    }

    #[test]
    fn parse_measurement_bad_humidity_crc() {
        let mut f = frame([0x66, 0x66], [0x80, 0x00]);
        f[5] ^= 0x01;
        assert_eq!(parse_measurement(&f), Err(CrcError::Humidity));
    }

    #[test]
    fn parse_measurement_reports_temperature_first_when_both_bad() {
        let mut f = frame([0x66, 0x66], [0x80, 0x00]);
        f[2] ^= 0x01;
        f[5] ^= 0x01;
        assert_eq!(parse_measurement(&f), Err(CrcError::Temperature));
    }

    #[test]
    fn parse_status_default_is_0x8010() {
        let s = parse_status(&[0x80, 0x10, 0xE1]).unwrap();
        assert_eq!(s.bits(), 0x8010);
        assert!(s.reset_detected());
        assert!(s.alert_pending());
        assert!(!s.heater_on());
        assert!(!s.command_failed());
        assert!(!s.checksum_failed());
    }

    #[test]
    fn status_bit_decode_each_bit() {
        let decode = |bits: u16| {
            let b = bits.to_be_bytes();
            parse_status(&[b[0], b[1], crc8(b)]).unwrap()
        };
        let s = decode(1 << 15);
        assert!(s.alert_pending() && !s.heater_on() && !s.reset_detected());
        let s = decode(1 << 13);
        assert!(s.heater_on() && !s.alert_pending() && !s.reset_detected());
        let s = decode(1 << 4);
        assert!(s.reset_detected() && !s.heater_on() && !s.command_failed());
        let s = decode(1 << 1);
        assert!(s.command_failed() && !s.checksum_failed() && !s.reset_detected());
        let s = decode(1);
        assert!(s.checksum_failed() && !s.command_failed() && !s.alert_pending());
        // Reserved bits are kept in bits() but set no flag.
        let s = decode(0b11_1110_0000);
        assert_eq!(s.bits(), 0b11_1110_0000);
        assert!(!s.alert_pending() && !s.heater_on() && !s.reset_detected());
    }

    #[test]
    fn parse_status_bad_crc_is_status_error() {
        assert_eq!(
            parse_status(&[0x80, 0x10, 0xE1 ^ 0x01]),
            Err(CrcError::Status)
        );
    }

    #[test]
    fn repeatability_commands_and_durations() {
        assert_eq!(Repeatability::High.command(), [0x24, 0x00]);
        assert_eq!(Repeatability::Medium.command(), [0x24, 0x0B]);
        assert_eq!(Repeatability::Low.command(), [0x24, 0x16]);
        assert_eq!(Repeatability::High.max_duration_us(), 15_500);
        assert_eq!(Repeatability::Medium.max_duration_us(), 6_500);
        assert_eq!(Repeatability::Low.max_duration_us(), 4_500);
    }

    #[test]
    fn command_constants_match_datasheet() {
        assert_eq!(CMD_SOFT_RESET, [0x30, 0xA2]);
        assert_eq!(CMD_HEATER_ON, [0x30, 0x6D]);
        assert_eq!(CMD_HEATER_OFF, [0x30, 0x66]);
        assert_eq!(CMD_READ_STATUS, [0xF3, 0x2D]);
        assert_eq!(CMD_CLEAR_STATUS, [0x30, 0x41]);
        assert_eq!(I2C_ADDR, 0x44);
        assert_eq!(I2C_ADDR_ALT, 0x45);
    }

    #[test]
    fn crc_error_display_names_the_word() {
        extern crate std;
        use std::string::ToString;
        assert!(CrcError::Temperature.to_string().contains("temperature"));
        assert!(CrcError::Humidity.to_string().contains("humidity"));
        assert!(CrcError::Status.to_string().contains("status"));
    }
}

#[cfg(test)]
mod machine_tests {
    use super::*;

    const ALL: [Repeatability; 3] = [
        Repeatability::High,
        Repeatability::Medium,
        Repeatability::Low,
    ];

    fn good_frame() -> [u8; 6] {
        let t = [0x66, 0x66];
        let rh = [0x80, 0x00];
        [t[0], t[1], crc8(t), rh[0], rh[1], crc8(rh)]
    }

    #[test]
    fn single_shot_happy_path_action_order() {
        let mut m = SingleShot::new(Repeatability::Medium);
        assert!(!m.is_finished());
        assert_eq!(
            m.poll(ShotInput::Start),
            ShotAction::WriteCommand([0x24, 0x0B])
        );
        assert_eq!(m.poll(ShotInput::Written), ShotAction::Wait { us: 6_500 });
        assert_eq!(m.poll(ShotInput::Waited), ShotAction::ReadFrame);
        assert!(!m.is_finished());
        let ShotAction::Done(reading) = m.poll(ShotInput::Frame(good_frame())) else {
            panic!("expected Done");
        };
        assert_eq!(reading.temperature_centi_c(), 2_500);
        assert!(m.is_finished());
    }

    #[test]
    fn wait_us_matches_repeatability_for_each_variant() {
        for r in ALL {
            let mut m = SingleShot::new(r);
            assert_eq!(
                m.poll(ShotInput::Start),
                ShotAction::WriteCommand(r.command())
            );
            assert_eq!(
                m.poll(ShotInput::Written),
                ShotAction::Wait {
                    us: r.max_duration_us()
                }
            );
        }
    }

    #[test]
    fn bad_crc_frame_yields_done_err_with_word() {
        let mut bad_t = good_frame();
        bad_t[2] ^= 0xFF;
        let mut bad_rh = good_frame();
        bad_rh[5] ^= 0xFF;
        for (f, word) in [(bad_t, CrcError::Temperature), (bad_rh, CrcError::Humidity)] {
            let mut m = SingleShot::new(Repeatability::High);
            let _ = m.poll(ShotInput::Start);
            let _ = m.poll(ShotInput::Written);
            let _ = m.poll(ShotInput::Waited);
            assert_eq!(m.poll(ShotInput::Frame(f)), ShotAction::Fail(word));
            assert!(m.is_finished());
        }
    }

    #[test]
    fn out_of_phase_matrix_leaves_state_unchanged() {
        let inputs = [
            ShotInput::Start,
            ShotInput::Written,
            ShotInput::Waited,
            ShotInput::Frame(good_frame()),
        ];
        // One machine per non-terminal phase, each with its matching input.
        let mut idle = SingleShot::new(Repeatability::High);
        let mut writing = idle;
        let _ = writing.poll(ShotInput::Start);
        let mut waiting = writing;
        let _ = waiting.poll(ShotInput::Written);
        let mut reading = waiting;
        let _ = reading.poll(ShotInput::Waited);
        let cases = [
            (&mut idle, ShotInput::Start),
            (&mut writing, ShotInput::Written),
            (&mut waiting, ShotInput::Waited),
            (&mut reading, ShotInput::Frame(good_frame())),
        ];
        for (m, right) in cases {
            for i in inputs {
                if i == right {
                    continue;
                }
                let before = *m;
                assert_eq!(m.poll(i), ShotAction::OutOfPhase);
                assert_eq!(*m, before);
            }
            assert_ne!(m.poll(right), ShotAction::OutOfPhase);
        }
    }

    #[test]
    fn terminal_is_sticky_after_done_and_after_failed_frame() {
        let mut bad = good_frame();
        bad[2] ^= 0xFF;
        for f in [good_frame(), bad] {
            let mut m = SingleShot::new(Repeatability::Low);
            let _ = m.poll(ShotInput::Start);
            let _ = m.poll(ShotInput::Written);
            let _ = m.poll(ShotInput::Waited);
            assert!(matches!(
                m.poll(ShotInput::Frame(f)),
                ShotAction::Done(_) | ShotAction::Fail(_)
            ));
            assert!(m.is_finished());
            for i in [
                ShotInput::Start,
                ShotInput::Written,
                ShotInput::Waited,
                ShotInput::Frame(f),
            ] {
                assert_eq!(m.poll(i), ShotAction::OutOfPhase);
            }
            assert!(m.is_finished());
        }
    }

    #[test]
    fn drive_completes_within_max_polls() {
        for r in ALL {
            let mut m = SingleShot::new(r);
            let mut input = ShotInput::Start;
            let mut polls = 0;
            loop {
                polls += 1;
                input = match m.poll(input) {
                    ShotAction::WriteCommand(_) => ShotInput::Written,
                    ShotAction::Wait { .. } => ShotInput::Waited,
                    ShotAction::ReadFrame => ShotInput::Frame(good_frame()),
                    ShotAction::Done(_) => break,
                    ShotAction::Fail(e) => panic!("unexpected CRC failure: {e}"),
                    ShotAction::OutOfPhase => panic!("out of phase"),
                };
            }
            assert_eq!(polls, 4);
            assert!(polls <= m.max_polls());
        }
    }

    #[test]
    fn recovering_waits_out_the_in_flight_command_before_writing() {
        let mut m = SingleShot::recovering(Repeatability::Low, Repeatability::High);
        assert_eq!(m.poll(ShotInput::Start), ShotAction::Wait { us: 15_500 });
        assert_eq!(
            m.poll(ShotInput::Waited),
            ShotAction::WriteCommand([0x24, 0x16])
        );
        assert_eq!(m.poll(ShotInput::Written), ShotAction::Wait { us: 4_500 });
        assert_eq!(m.poll(ShotInput::Waited), ShotAction::ReadFrame);
        assert!(!m.is_finished());
    }

    #[test]
    fn recovering_run_stays_within_the_poll_bound() {
        let mut m = SingleShot::recovering(Repeatability::High, Repeatability::High);
        let mut input = ShotInput::Start;
        let mut polls = 0;
        loop {
            polls += 1;
            input = match m.poll(input) {
                ShotAction::WriteCommand(_) => ShotInput::Written,
                ShotAction::Wait { .. } => ShotInput::Waited,
                ShotAction::ReadFrame => ShotInput::Frame(good_frame()),
                ShotAction::Done(_) => break,
                ShotAction::Fail(e) => panic!("crc {e:?}"),
                ShotAction::OutOfPhase => panic!("out of phase"),
            };
        }
        assert_eq!(polls, 5);
        assert!(polls <= m.max_polls());
    }

    #[test]
    fn fresh_machine_rejects_waited_and_recovering_rejects_written() {
        let mut fresh = SingleShot::new(Repeatability::High);
        assert_eq!(fresh.poll(ShotInput::Waited), ShotAction::OutOfPhase);
        let mut rec = SingleShot::recovering(Repeatability::High, Repeatability::Low);
        assert_eq!(rec.poll(ShotInput::Written), ShotAction::OutOfPhase);
        assert_eq!(rec.poll(ShotInput::Start), ShotAction::Wait { us: 4_500 });
    }
}

#[cfg(all(test, feature = "hal"))]
mod driver {
    use super::*;
    use crate::mock::{MockI2c, MockI2cError, NoopDelay, Transaction};
    use core::cell::Cell;
    use embedded_hal::i2c::{ErrorType, Operation, SevenBitAddress};

    const A: u8 = I2C_ADDR;
    const STATUS_DEFAULT: [u8; 3] = [0x80, 0x10, 0xE1];

    fn frame(t: [u8; 2], rh: [u8; 2]) -> [u8; 6] {
        [t[0], t[1], crc8(t), rh[0], rh[1], crc8(rh)]
    }

    fn init_script(address: u8, status: &[u8]) -> [Transaction<'_>; 3] {
        [
            Transaction::Write {
                address,
                bytes: &CMD_SOFT_RESET,
            },
            Transaction::Write {
                address,
                bytes: &CMD_READ_STATUS,
            },
            Transaction::Read {
                address,
                response: status,
            },
        ]
    }

    /// Records every `DelayNs` call so a test can assert the total.
    struct CountingDelay {
        total_ns: u64,
    }

    impl embedded_hal::delay::DelayNs for CountingDelay {
        fn delay_ns(&mut self, ns: u32) {
            self.total_ns += u64::from(ns);
        }
    }

    fn counting() -> CountingDelay {
        CountingDelay { total_ns: 0 }
    }

    #[test]
    fn init_script_reset_write_then_status_write_then_read() {
        let script = init_script(A, &STATUS_DEFAULT);
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let mut d = counting();
        let st = s.init(&mut d).unwrap();
        assert_eq!(st.bits(), 0x8010);
        // 2 ms power-up + 2 ms soft reset + 1 ms command gap.
        assert_eq!(d.total_ns, 5_000_000);
        s.release().done();
    }

    #[test]
    fn init_returns_status_without_asserting_reset_bit() {
        // A stale reset-detected bit (bit 4 clear) is not a fault.
        let status = [0x80, 0x00, crc8([0x80, 0x00])];
        let script = init_script(A, &status);
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let st = s.init(&mut NoopDelay).unwrap();
        assert!(!st.reset_detected());
        s.release().done();
    }

    #[test]
    fn init_nack_on_reset_keeps_bus_error_and_is_nack() {
        let script = [Transaction::Error(MockI2cError::nack())];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let e = s.init(&mut NoopDelay).unwrap_err();
        assert!(matches!(e, Error::Bus(_)));
        assert!(e.is_nack());
        s.release().done();
    }

    #[test]
    fn init_bus_fault_on_status_is_not_nack() {
        let script = [
            Transaction::Write {
                address: A,
                bytes: &CMD_SOFT_RESET,
            },
            Transaction::Write {
                address: A,
                bytes: &CMD_READ_STATUS,
            },
            Transaction::Error(MockI2cError::bus()),
        ];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let e = s.init(&mut NoopDelay).unwrap_err();
        assert!(matches!(e, Error::Bus(_)));
        assert!(!e.is_nack());
        s.release().done();
    }

    #[test]
    fn init_crc_failure_on_status_is_crc_status() {
        let bad = [0x80, 0x10, 0xE1 ^ 0xFF];
        let script = init_script(A, &bad);
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        assert_eq!(s.init(&mut NoopDelay), Err(Error::Crc(CrcError::Status)));
        s.release().done();
    }

    #[test]
    fn soft_reset_standalone_waits_two_ms_after_write() {
        let script = [Transaction::Write {
            address: A,
            bytes: &CMD_SOFT_RESET,
        }];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let mut d = counting();
        s.soft_reset(&mut d).unwrap();
        assert_eq!(d.total_ns, 2_000_000);
        s.release().done();
    }

    #[test]
    fn status_standalone_waits_one_ms() {
        let script = [
            Transaction::Write {
                address: A,
                bytes: &CMD_READ_STATUS,
            },
            Transaction::Read {
                address: A,
                response: &STATUS_DEFAULT,
            },
        ];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let mut d = counting();
        assert_eq!(s.status(&mut d).unwrap().bits(), 0x8010);
        assert_eq!(d.total_ns, 1_000_000);
        s.release().done();
    }

    #[test]
    fn measure_sends_command_waits_then_reads_once() {
        for r in [
            Repeatability::High,
            Repeatability::Medium,
            Repeatability::Low,
        ] {
            let f = frame([0x66, 0x66], [0x80, 0x00]);
            let cmd = r.command();
            let script = [
                Transaction::Write {
                    address: A,
                    bytes: &cmd,
                },
                Transaction::Read {
                    address: A,
                    response: &f,
                },
            ];
            let mut s = Sht3x::new(MockI2c::new(&script), A);
            let mut d = counting();
            let m = s.measure(&mut d, r).unwrap();
            assert_eq!(m.temperature_centi_c(), 2_500);
            assert_eq!(m.humidity_centi_rh(), 5_000);
            assert_eq!(d.total_ns, u64::from(r.max_duration_us()) * 1_000);
            s.release().done();
        }
    }

    fn measure_with_frame(f: [u8; 6]) -> Result<Measurement, Error<MockI2cError>> {
        let script = [
            Transaction::Write {
                address: A,
                bytes: &[0x24, 0x00],
            },
            Transaction::Read {
                address: A,
                response: &f,
            },
        ];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let r = s.measure(&mut NoopDelay, Repeatability::High);
        s.release().done();
        r
    }

    #[test]
    fn measure_crc_failure_on_temperature_word() {
        let mut f = frame([0x66, 0x66], [0x80, 0x00]);
        f[2] ^= 0xFF;
        assert_eq!(
            measure_with_frame(f),
            Err(Error::Crc(CrcError::Temperature))
        );
    }

    #[test]
    fn measure_crc_failure_on_humidity_word() {
        let mut f = frame([0x66, 0x66], [0x80, 0x00]);
        f[5] ^= 0xFF;
        assert_eq!(measure_with_frame(f), Err(Error::Crc(CrcError::Humidity)));
    }

    #[test]
    fn measure_nack_on_read_is_reported_not_retried() {
        let script = [
            Transaction::Write {
                address: A,
                bytes: &[0x24, 0x00],
            },
            Transaction::Error(MockI2cError::nack()),
        ];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let e = s.measure(&mut NoopDelay, Repeatability::High).unwrap_err();
        assert!(e.is_nack());
        // A retry would have panicked on the exhausted script.
        s.release().done();
    }

    #[test]
    fn measure_bus_fault_on_write() {
        let script = [Transaction::Error(MockI2cError::bus())];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let e = s.measure(&mut NoopDelay, Repeatability::Low).unwrap_err();
        assert!(matches!(e, Error::Bus(_)));
        assert!(!e.is_nack());
        s.release().done();
    }

    #[test]
    fn operates_at_alternate_address_0x45() {
        let f = frame([0x66, 0x66], [0x80, 0x00]);
        let script = [
            Transaction::Write {
                address: I2C_ADDR_ALT,
                bytes: &CMD_SOFT_RESET,
            },
            Transaction::Write {
                address: I2C_ADDR_ALT,
                bytes: &CMD_READ_STATUS,
            },
            Transaction::Read {
                address: I2C_ADDR_ALT,
                response: &STATUS_DEFAULT,
            },
            Transaction::Write {
                address: I2C_ADDR_ALT,
                bytes: &[0x24, 0x00],
            },
            Transaction::Read {
                address: I2C_ADDR_ALT,
                response: &f,
            },
        ];
        let mut s = Sht3x::new(MockI2c::new(&script), I2C_ADDR_ALT);
        s.init(&mut NoopDelay).unwrap();
        s.measure(&mut NoopDelay, Repeatability::High).unwrap();
        s.release().done();
    }

    // ── 1 ms command spacing across public calls ────────────────────────

    /// Shared virtual clock, advanced only by [`TimedDelay`].
    struct TimedDelay<'c> {
        now_ns: &'c Cell<u64>,
    }

    impl embedded_hal::delay::DelayNs for TimedDelay<'_> {
        fn delay_ns(&mut self, ns: u32) {
            self.now_ns.set(self.now_ns.get() + u64::from(ns));
        }
    }

    /// Wraps [`MockI2c`] and stamps the virtual time of every write (command).
    struct TimedBus<'a, 'c> {
        inner: MockI2c<'a>,
        now_ns: &'c Cell<u64>,
        write_times: [u64; 8],
        writes: usize,
    }

    impl ErrorType for TimedBus<'_, '_> {
        type Error = MockI2cError;
    }

    impl embedded_hal::i2c::I2c<SevenBitAddress> for TimedBus<'_, '_> {
        fn transaction(
            &mut self,
            address: SevenBitAddress,
            operations: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            if matches!(operations.first(), Some(Operation::Write(_))) {
                self.write_times[self.writes] = self.now_ns.get();
                self.writes += 1;
            }
            self.inner.transaction(address, operations)
        }
    }

    #[test]
    fn no_two_commands_closer_than_one_ms_across_init_then_measure() {
        let f = frame([0x66, 0x66], [0x80, 0x00]);
        let script = [
            Transaction::Write {
                address: A,
                bytes: &CMD_SOFT_RESET,
            },
            Transaction::Write {
                address: A,
                bytes: &CMD_READ_STATUS,
            },
            Transaction::Read {
                address: A,
                response: &STATUS_DEFAULT,
            },
            Transaction::Write {
                address: A,
                bytes: &[0x24, 0x00],
            },
            Transaction::Read {
                address: A,
                response: &f,
            },
        ];
        let now = Cell::new(0);
        let bus = TimedBus {
            inner: MockI2c::new(&script),
            now_ns: &now,
            write_times: [0; 8],
            writes: 0,
        };
        let mut s = Sht3x::new(bus, A);
        let mut d = TimedDelay { now_ns: &now };
        s.init(&mut d).unwrap();
        s.measure(&mut d, Repeatability::High).unwrap();
        let bus = s.release();
        bus.inner.done();
        // Reset, status command, measurement command.
        assert_eq!(bus.writes, 3);
        for pair in bus.write_times[..bus.writes].windows(2) {
            assert!(pair[1] - pair[0] >= 1_000_000, "{pair:?}");
        }
    }

    #[test]
    fn new_touches_no_bus_and_release_returns_bus() {
        let script = [Transaction::Write {
            address: A,
            bytes: &CMD_SOFT_RESET,
        }];
        let s = Sht3x::new(MockI2c::new(&script), A);
        let bus = s.release();
        assert_eq!(bus.remaining(), 1);
        let mut s = Sht3x::new(bus, A);
        s.soft_reset(&mut NoopDelay).unwrap();
        s.release().done();
    }

    #[test]
    fn error_display_and_is_nack_matrix() {
        extern crate std;
        use std::string::ToString;
        let bus = Error::Bus(MockI2cError::bus());
        assert!(bus.to_string().contains("I2C bus error"));
        assert!(!bus.is_nack());
        let nack = Error::Bus(MockI2cError::nack());
        assert!(nack.is_nack());
        let crc = Error::<MockI2cError>::Crc(CrcError::Humidity);
        assert!(crc.to_string().contains("humidity"));
        assert!(!crc.is_nack());
        assert_eq!(
            Error::<MockI2cError>::from(CrcError::Status),
            Error::Crc(CrcError::Status)
        );
        let seq = Error::<MockI2cError>::Sequence;
        assert!(seq.to_string().contains("fell out of step"));
        assert!(seq.to_string().contains("library bug"));
        assert!(!seq.is_nack());
    }

    #[test]
    fn not_ready_read_leaves_the_command_in_flight_and_the_retry_waits_it_out() {
        let cmd = Repeatability::High.command();
        let f = frame([0x66, 0x66], [0x80, 0x00]);
        let script = [
            Transaction::Write {
                address: A,
                bytes: &cmd,
            },
            Transaction::Error(MockI2cError::nack()),
            Transaction::Write {
                address: A,
                bytes: &cmd,
            },
            Transaction::Read {
                address: A,
                response: &f,
            },
        ];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let mut d = counting();
        assert!(s
            .measure(&mut d, Repeatability::High)
            .unwrap_err()
            .is_nack());
        assert_eq!(d.total_ns, 15_500_000);
        let m = s.measure(&mut d, Repeatability::High).unwrap();
        assert_eq!(m.temperature_centi_c(), 2_500);
        // The in-flight High command is waited out, then the new conversion.
        assert_eq!(d.total_ns, 3 * 15_500_000);
        s.release().done();
    }

    #[test]
    fn status_after_a_not_ready_read_waits_out_the_command_first() {
        let cmd = Repeatability::Low.command();
        let script = [
            Transaction::Write {
                address: A,
                bytes: &cmd,
            },
            Transaction::Error(MockI2cError::nack()),
            Transaction::Write {
                address: A,
                bytes: &CMD_READ_STATUS,
            },
            Transaction::Read {
                address: A,
                response: &STATUS_DEFAULT,
            },
        ];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let mut d = counting();
        assert!(s.measure(&mut d, Repeatability::Low).is_err());
        d.total_ns = 0;
        s.status(&mut d).unwrap();
        // 4.5 ms for the Low command in flight, then the 1 ms command gap.
        assert_eq!(d.total_ns, 5_500_000);
        s.release().done();
    }

    #[test]
    fn successful_measurements_add_no_recovery_wait() {
        let cmd = Repeatability::Medium.command();
        let f = frame([0x66, 0x66], [0x80, 0x00]);
        let script = [
            Transaction::Write {
                address: A,
                bytes: &cmd,
            },
            Transaction::Read {
                address: A,
                response: &f,
            },
            Transaction::Write {
                address: A,
                bytes: &cmd,
            },
            Transaction::Read {
                address: A,
                response: &f,
            },
        ];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let mut d = counting();
        s.measure(&mut d, Repeatability::Medium).unwrap();
        s.measure(&mut d, Repeatability::Medium).unwrap();
        assert_eq!(d.total_ns, 2 * 6_500_000);
        s.release().done();
    }

    #[test]
    fn refused_write_clears_the_in_flight_command() {
        // An absent sensor NACKs every command: no conversion runs, so no wait.
        let script = [
            Transaction::Error(MockI2cError::nack()),
            Transaction::Error(MockI2cError::nack()),
        ];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let mut d = counting();
        assert!(s
            .measure(&mut d, Repeatability::High)
            .unwrap_err()
            .is_nack());
        assert!(s
            .measure(&mut d, Repeatability::High)
            .unwrap_err()
            .is_nack());
        assert_eq!(d.total_ns, 0);
        s.release().done();
    }

    #[test]
    fn ambiguous_write_fault_keeps_the_command_in_flight() {
        let cmd = Repeatability::High.command();
        let f = frame([0x66, 0x66], [0x80, 0x00]);
        let script = [
            Transaction::Error(MockI2cError::bus()),
            Transaction::Write {
                address: A,
                bytes: &cmd,
            },
            Transaction::Read {
                address: A,
                response: &f,
            },
        ];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let mut d = counting();
        assert!(matches!(
            s.measure(&mut d, Repeatability::High),
            Err(Error::Bus(_))
        ));
        s.measure(&mut d, Repeatability::High).unwrap();
        // The faulted write may have landed: wait it out, then the new conversion.
        assert_eq!(d.total_ns, 2 * 15_500_000);
        s.release().done();
    }

    #[test]
    fn soft_reset_bus_fault_still_waits_before_the_next_command() {
        // The reset may have reached the sensor: wait t_SR out, then the
        // status command follows with its own gap.
        let script = [
            Transaction::Error(MockI2cError::bus()),
            Transaction::Write {
                address: A,
                bytes: &CMD_READ_STATUS,
            },
            Transaction::Read {
                address: A,
                response: &STATUS_DEFAULT,
            },
        ];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let mut d = counting();
        assert!(matches!(s.soft_reset(&mut d), Err(Error::Bus(_))));
        assert_eq!(d.total_ns, 2_000_000);
        s.status(&mut d).unwrap();
        assert_eq!(d.total_ns, 3_000_000);
        s.release().done();
    }

    #[test]
    fn soft_reset_nack_returns_without_waiting() {
        let script = [Transaction::Error(MockI2cError::nack())];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let mut d = counting();
        assert!(s.soft_reset(&mut d).unwrap_err().is_nack());
        assert_eq!(d.total_ns, 0);
        s.release().done();
    }

    #[test]
    fn status_write_bus_fault_still_waits_the_command_gap() {
        let script = [
            Transaction::Error(MockI2cError::bus()),
            Transaction::Write {
                address: A,
                bytes: &CMD_SOFT_RESET,
            },
        ];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let mut d = counting();
        assert!(matches!(s.status(&mut d), Err(Error::Bus(_))));
        assert_eq!(d.total_ns, 1_000_000);
        s.soft_reset(&mut d).unwrap();
        assert_eq!(d.total_ns, 3_000_000);
        s.release().done();
    }
}

// ── Async runner parity ─────────────────────────────────────────────────

#[cfg(all(test, feature = "hal-async"))]
mod parity {
    use super::*;
    use crate::mock::{block_on, MockI2c, MockI2cError, Transaction};
    use core::future::Future;
    use core::pin::{pin, Pin};
    use core::task::{Context, Poll, Waker};

    const A: u8 = I2C_ADDR;

    /// Counts time through both the blocking and the async `DelayNs`.
    struct BothDelay {
        total_ns: u64,
    }

    impl embedded_hal_async::delay::DelayNs for BothDelay {
        async fn delay_ns(&mut self, ns: u32) {
            self.total_ns += u64::from(ns);
        }
    }

    impl embedded_hal::delay::DelayNs for BothDelay {
        fn delay_ns(&mut self, ns: u32) {
            self.total_ns += u64::from(ns);
        }
    }

    fn frame(t: [u8; 2], rh: [u8; 2]) -> [u8; 6] {
        [t[0], t[1], crc8(t), rh[0], rh[1], crc8(rh)]
    }

    /// Runs `script` through both runners (each must consume it in full)
    /// and returns the shared result.
    fn both(
        script: &[Transaction<'_>],
        r: Repeatability,
    ) -> Result<Measurement, Error<MockI2cError>> {
        let mut blocking = Sht3x::new(MockI2c::new(script), A);
        let mut bd = BothDelay { total_ns: 0 };
        let sync = blocking.measure(&mut bd, r);
        blocking.release().done();

        let mut asynch = Sht3x::new(MockI2c::new(script), A);
        let mut ad = BothDelay { total_ns: 0 };
        let a = block_on(asynch.measure_async(&mut ad, r));
        asynch.release().done();

        assert_eq!(sync, a);
        assert_eq!(bd.total_ns, ad.total_ns);
        if a.is_ok() {
            assert_eq!(ad.total_ns, u64::from(r.max_duration_us()) * 1_000);
        }
        a
    }

    fn script_for<'a>(cmd: &'a [u8; 2], f: &'a [u8; 6]) -> [Transaction<'a>; 2] {
        [
            Transaction::Write {
                address: A,
                bytes: cmd,
            },
            Transaction::Read {
                address: A,
                response: f,
            },
        ]
    }

    #[test]
    fn parity_happy_path_each_repeatability() {
        for r in [
            Repeatability::High,
            Repeatability::Medium,
            Repeatability::Low,
        ] {
            let f = frame([0x74, 0x4B], [0x80, 0x00]);
            let cmd = r.command();
            let m = both(&script_for(&cmd, &f), r).unwrap();
            assert_eq!(m.temperature_centi_c(), 3_450);
        }
    }

    #[test]
    fn parity_crc_failure_temperature() {
        let mut f = frame([0x66, 0x66], [0x80, 0x00]);
        f[2] ^= 0xFF;
        let cmd = Repeatability::High.command();
        assert_eq!(
            both(&script_for(&cmd, &f), Repeatability::High),
            Err(Error::Crc(CrcError::Temperature))
        );
    }

    #[test]
    fn parity_crc_failure_humidity() {
        let mut f = frame([0x66, 0x66], [0x80, 0x00]);
        f[5] ^= 0xFF;
        let cmd = Repeatability::High.command();
        assert_eq!(
            both(&script_for(&cmd, &f), Repeatability::High),
            Err(Error::Crc(CrcError::Humidity))
        );
    }

    #[test]
    fn parity_nack_on_read() {
        let cmd = Repeatability::Medium.command();
        let script = [
            Transaction::Write {
                address: A,
                bytes: &cmd,
            },
            Transaction::Error(MockI2cError::nack()),
        ];
        let e = both(&script, Repeatability::Medium).unwrap_err();
        assert!(e.is_nack());
    }

    #[test]
    fn parity_bus_fault_on_write() {
        let script = [Transaction::Error(MockI2cError::bus())];
        assert!(matches!(
            both(&script, Repeatability::Low),
            Err(Error::Bus(_))
        ));
    }

    #[test]
    fn parity_refused_write_clears_in_flight_in_both_runners() {
        let script = [
            Transaction::Error(MockI2cError::nack()),
            Transaction::Error(MockI2cError::nack()),
        ];
        let mut blocking = Sht3x::new(MockI2c::new(&script), A);
        let mut bd = BothDelay { total_ns: 0 };
        assert!(blocking.measure(&mut bd, Repeatability::High).is_err());
        assert!(blocking.measure(&mut bd, Repeatability::High).is_err());
        blocking.release().done();

        let mut asynch = Sht3x::new(MockI2c::new(&script), A);
        let mut ad = BothDelay { total_ns: 0 };
        assert!(block_on(asynch.measure_async(&mut ad, Repeatability::High)).is_err());
        assert!(block_on(asynch.measure_async(&mut ad, Repeatability::High)).is_err());
        asynch.release().done();

        assert_eq!((bd.total_ns, ad.total_ns), (0, 0));
    }

    /// A bus that implements only the async `I2c`, like an embassy shared-bus device.
    struct AsyncOnly<'a>(MockI2c<'a>);

    impl embedded_hal::i2c::ErrorType for AsyncOnly<'_> {
        type Error = MockI2cError;
    }

    impl embedded_hal_async::i2c::I2c for AsyncOnly<'_> {
        async fn transaction(
            &mut self,
            address: u8,
            operations: &mut [embedded_hal::i2c::Operation<'_>],
        ) -> Result<(), Self::Error> {
            embedded_hal_async::i2c::I2c::transaction(&mut self.0, address, operations).await
        }
    }

    #[test]
    fn async_only_bus_constructs_measures_and_releases() {
        let f = frame([0x66, 0x66], [0x80, 0x00]);
        let cmd = Repeatability::High.command();
        let script = script_for(&cmd, &f);
        let mut s = Sht3x::new(AsyncOnly(MockI2c::new(&script)), A);
        let mut d = BothDelay { total_ns: 0 };
        let m = block_on(s.measure_async(&mut d, Repeatability::High)).unwrap();
        assert_eq!(m.humidity_centi_rh(), 5_000);
        assert_eq!(d.total_ns, 15_500_000);
        s.release().0.done();
    }

    /// Future that is `Pending` on its first poll and `Ready` afterwards.
    struct Yield(bool);

    impl Future for Yield {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
            if self.0 {
                Poll::Ready(())
            } else {
                self.0 = true;
                Poll::Pending
            }
        }
    }

    /// Async delay that suspends once, so a test can drop a future mid-run.
    struct YieldOnce {
        yielded: bool,
        total_ns: u64,
    }

    impl embedded_hal_async::delay::DelayNs for YieldOnce {
        async fn delay_ns(&mut self, ns: u32) {
            self.total_ns += u64::from(ns);
            if !self.yielded {
                self.yielded = true;
                Yield(false).await;
            }
        }
    }

    #[test]
    fn cancelled_measure_async_makes_the_next_call_wait_out_the_command() {
        let cmd = Repeatability::High.command();
        let f = frame([0x66, 0x66], [0x80, 0x00]);
        // The first command goes out and the future is dropped inside the
        // conversion wait; the retry waits that command out before its own.
        let script = [
            Transaction::Write {
                address: A,
                bytes: &cmd,
            },
            Transaction::Write {
                address: A,
                bytes: &cmd,
            },
            Transaction::Read {
                address: A,
                response: &f,
            },
        ];
        let mut s = Sht3x::new(MockI2c::new(&script), A);
        let mut d = YieldOnce {
            yielded: false,
            total_ns: 0,
        };
        {
            let mut fut = pin!(s.measure_async(&mut d, Repeatability::High));
            let mut cx = Context::from_waker(Waker::noop());
            assert!(fut.as_mut().poll(&mut cx).is_pending());
        }
        assert_eq!(
            d.total_ns, 15_500_000,
            "suspended inside the conversion wait"
        );
        let m = block_on(s.measure_async(&mut d, Repeatability::High)).unwrap();
        assert_eq!(m.temperature_centi_c(), 2_500);
        assert_eq!(d.total_ns, 3 * 15_500_000);
        s.release().done();
    }
}
