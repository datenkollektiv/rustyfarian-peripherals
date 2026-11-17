//! Infrared transmit and receive on the ESP32 RMT peripheral (feature `ir`).
//!
//! [`IrTransmitter`] sends a pulse list (marks and spaces in microseconds, mark
//! first) as a modulated carrier on a pin.
//! [`IrReceiver`] captures one frame from a demodulated receiver module (TSOP,
//! VS1838, OPEN-SMART) and hands back the pulse list.
//! Both implement [`PulseSink`] / [`PulseSource`], so firmware can be written
//! against the traits and host-tested with `MockPulseSink` / `MockPulseSource`.
//! Both are thin bridges: encoding, decoding, carrier arithmetic and glitch
//! filtering live in [`tamer::ir`], which this module also re-exports.
//!
//! # Clocks
//!
//! The examples use [`IR_RMT_CLOCK`] (80 MHz) for `Rmt::new`, the one value all
//! four chips accept.
//! The channel divider is `rmt_clock / 1 MHz` (80), so one RMT tick is 1 us and
//! `tamer` durations map one to one.
//!
//! The TX carrier counts in the RMT *group* clock (the `Rmt::new` frequency),
//! not in the divided channel clock.
//! This is derived from the esp-hal 1.2.2 and ESP-IDF sources and is **not yet
//! confirmed on hardware**: check for 38 kHz / 33 % on a scope or a TSOP
//! (see `docs/features/ir-codec-v1.md`).
//! A carrier near 475 Hz would mean the channel clock is used instead.
//! The half-periods are written to `carrier_high` / `carrier_low` unadjusted
//! (esp-hal 1.2.2 applies no `- 1`), and whether the TRM counts `N` or `N + 1`
//! ticks is unverified: at most about 0.1 % off, to be covered by the same scope check.
//!
//! # Limits
//!
//! - Frame end is the RMT idle threshold, at most 32767 us at 1 us ticks, so
//!   the reference's 150 ms gap is not reachable.
//!   The default is 20 ms: above every internal gap of NEC and the candle
//!   remote, below the NEC repeat gap, so a repeat arrives as its own short
//!   frame.
//! - [`IrReceiver::receive`] and [`IrTransmitter::send`] block, and no frame is
//!   captured between two `receive` calls.
//!   A frame in flight at a call boundary can arrive truncated, so treat a
//!   decode error as droppable.
//! - The classic ESP32 has no RX wrap, so `CODES` must fit one channel RAM
//!   block there.
//!
//! The ESP32-C3 is the reference chip; the other chips are compile-verified only.
//!
//! # Example
//!
//! ```ignore
//! use rustyfarian_esp_hal_peripherals::ir::{
//!     IrReceiver, IrRxConfig, IrTransmitter, IrTxConfig, IR_RMT_CLOCK,
//! };
//! use esp_hal::rmt::Rmt;
//!
//! let p = esp_hal::init(esp_hal::Config::default());
//! let rmt = Rmt::new(p.RMT, IR_RMT_CLOCK).unwrap();
//! let clock = rmt.frequency();
//! let mut tx = IrTransmitter::<64>::new(rmt.channel0, p.GPIO10, clock, &IrTxConfig::default())?;
//! tx.send(tamer::ir::PulseCodec::nec().encode::<67>(0x00FF_00FF)?.as_slice())?;
//! ```

use core::fmt;

use esp_hal::{
    gpio::{
        interconnect::{PeripheralInput, PeripheralOutput},
        Level,
    },
    rmt::{PulseCode, RxChannelConfig, RxChannelCreator, TxChannelConfig, TxChannelCreator},
    time::{Duration, Instant, Rate},
    Blocking,
};

pub use tamer::ir::*;

/// RMT group clock the examples pass to `Rmt::new`: 80 MHz, accepted by all four chips.
pub const IR_RMT_CLOCK: Rate = Rate::from_mhz(80);

/// Longest RMT pulse at 1 us ticks (15-bit length field).
const MAX_PULSE_US: u32 = 0x7FFF;

/// One RMT tick in the bridge is one microsecond.
const TICK_HZ: u32 = 1_000_000;

/// Divider turning `rmt_clock` into 1 us ticks, or `None` if not exact in `1..=255`.
fn tick_divider(rmt_clock: Rate) -> Option<u8> {
    let hz = rmt_clock.as_hz();
    if !hz.is_multiple_of(TICK_HZ) {
        return None;
    }
    u8::try_from(hz / TICK_HZ).ok().filter(|&d| d != 0)
}

/// Error returned by [`IrTransmitter`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IrTxError {
    /// The RMT clock or channel configuration was rejected.
    Config,
    /// The carrier does not fit the 16-bit carrier registers at this clock.
    Carrier,
    /// The pulse list needs more RMT codes than `CODES`.
    Overflow,
    /// A pulse is zero or longer than the 15-bit RMT limit (32767 us).
    InvalidPulse,
    /// The RMT peripheral reported a transmit error.
    Hardware,
}

impl fmt::Display for IrTxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Config => "IR TX configuration rejected",
            Self::Carrier => "IR carrier out of range for the RMT clock",
            Self::Overflow => "IR pulse list too long for the buffer",
            Self::InvalidPulse => "IR pulse is zero or longer than 32767 us",
            Self::Hardware => "IR transmit failed",
        })
    }
}

impl core::error::Error for IrTxError {}

/// Transmit settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrTxConfig {
    carrier_hz: u32,
    duty_percent: u8,
}

impl IrTxConfig {
    /// Carrier frequency in Hz and duty (percent of the period high).
    #[must_use]
    pub const fn new(carrier_hz: u32, duty_percent: u8) -> Self {
        Self {
            carrier_hz,
            duty_percent,
        }
    }
}

impl Default for IrTxConfig {
    /// 38 kHz, 33 % duty: the consumer-IR standard.
    fn default() -> Self {
        Self::new(38_000, 33)
    }
}

/// Blocking infrared transmitter on one RMT TX channel.
///
/// `CODES` is the stack buffer used per [`send`](Self::send), in RMT codes (two
/// pulses each); an NEC frame needs 35.
/// The carrier gates the high (mark) level and the pin idles low, so the LED
/// rests off.
pub struct IrTransmitter<'d, const CODES: usize = 64> {
    channel: esp_hal::rmt::Channel<'d, Blocking, esp_hal::rmt::Tx>,
}

impl<'d, const CODES: usize> IrTransmitter<'d, CODES> {
    /// Take the TX channel creator and the output pin.
    ///
    /// `rmt_clock` is `Rmt::frequency()`, read before moving the creator out of
    /// the `Rmt`.
    ///
    /// # Errors
    ///
    /// [`IrTxError::Config`] if the clock is not an exact multiple of 1 MHz up
    /// to 255 MHz or the channel cannot be configured, [`IrTxError::Carrier`]
    /// if the carrier does not fit at this clock.
    pub fn new(
        channel: impl TxChannelCreator<'d, Blocking>,
        pin: impl PeripheralOutput<'d>,
        rmt_clock: Rate,
        config: &IrTxConfig,
    ) -> Result<Self, IrTxError> {
        let divider = tick_divider(rmt_clock).ok_or(IrTxError::Config)?;
        let (high, low) = carrier_ticks(rmt_clock.as_hz(), config.carrier_hz, config.duty_percent)
            .ok_or(IrTxError::Carrier)?;
        let cfg = TxChannelConfig::default()
            .with_clk_divider(divider)
            .with_carrier_modulation(true)
            // Counted in the RMT group clock, not the channel clock (see module docs).
            .with_carrier_high(high)
            .with_carrier_low(low)
            // The esp-hal default is Low, which would carry the space, not the mark.
            .with_carrier_level(Level::High)
            .with_idle_output(true)
            .with_idle_output_level(Level::Low)
            .with_memsize(1);
        let channel = channel
            .configure_tx(&cfg)
            .map_err(|_| IrTxError::Config)?
            .with_pin(pin);
        Ok(Self { channel })
    }

    /// Send one pulse list (microseconds, alternating mark and space, mark
    /// first, odd length allowed) and busy-wait until it is out.
    ///
    /// Blocks for the frame duration (about 70 ms for NEC).
    ///
    /// # Errors
    ///
    /// [`IrTxError::Overflow`] if the list does not fit `CODES`,
    /// [`IrTxError::InvalidPulse`] for an empty list or a zero or over-long pulse,
    /// [`IrTxError::Hardware`] if the RMT reports a transmit error.
    pub fn send(&mut self, pulses: &[u32]) -> Result<(), IrTxError> {
        validate_pulses(pulses, MAX_PULSE_US).map_err(|_| IrTxError::InvalidPulse)?;
        let mut buf = [PulseCode::end_marker(); CODES];
        let mut n = 0;
        for (mark, space) in pulse_pairs(pulses) {
            let code = PulseCode::try_new(Level::High, mark, Level::Low, space)
                .ok_or(IrTxError::InvalidPulse)?;
            *buf.get_mut(n).ok_or(IrTxError::Overflow)? = code;
            n += 1;
        }
        // An even-length list needs an explicit end marker; an odd one ends on its zero space.
        if pulses.len().is_multiple_of(2) {
            buf.get_mut(n).ok_or(IrTxError::Overflow)?;
            n += 1;
        }
        let transaction = self
            .channel
            .reborrow()
            .transmit(&buf[..n])
            .map_err(|_| IrTxError::Hardware)?;
        transaction
            .wait()
            .map(|_| ())
            .map_err(|_| IrTxError::Hardware)
    }
}

/// Error returned by [`IrReceiver`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IrRxError {
    /// The RMT clock or channel configuration was rejected.
    Config,
    /// The frame did not fit the receive buffers and was dropped.
    Overflow,
    /// The RMT peripheral reported a receive error.
    Hardware,
}

impl fmt::Display for IrRxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Config => "IR RX configuration rejected",
            Self::Overflow => "IR frame too long for the buffer",
            Self::Hardware => "IR receive failed",
        })
    }
}

impl core::error::Error for IrRxError {}

/// Receive settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrRxConfig {
    idle_us: u16,
    min_pulse_us: u32,
}

impl IrRxConfig {
    /// `idle_us` is the quiet time that ends a frame (valid range `1..=32767`;
    /// [`IrReceiver::new`] rejects 0, and values above 32767 are rejected as
    /// [`IrRxError::Config`]), and
    /// `min_pulse_us` the glitch threshold applied by `tamer`'s assembler.
    #[must_use]
    pub const fn new(idle_us: u16, min_pulse_us: u32) -> Self {
        Self {
            idle_us,
            min_pulse_us,
        }
    }
}

impl Default for IrRxConfig {
    /// 20 ms frame-end idle, 20 us glitch filter.
    fn default() -> Self {
        Self::new(20_000, 20)
    }
}

/// Blocking infrared receiver on one RMT RX channel, for an active-low
/// demodulated module (output low while a mark is received).
///
/// `CODES` is the RMT capture buffer (two pulses each, on the stack during
/// [`receive`](Self::receive)); `PULSES` is the frame buffer held by the
/// receiver and should be at least `2 * CODES`.
/// Demodulation in the RMT stays off: the module already demodulates.
pub struct IrReceiver<'d, const CODES: usize = 64, const PULSES: usize = 128> {
    channel: esp_hal::rmt::Channel<'d, Blocking, esp_hal::rmt::Rx>,
    assembler: FrameAssembler<PULSES>,
}

impl<'d, const CODES: usize, const PULSES: usize> IrReceiver<'d, CODES, PULSES> {
    /// Take the RX channel creator and the input pin.
    ///
    /// `rmt_clock` is `Rmt::frequency()`, read before moving the creator out of
    /// the `Rmt`.
    ///
    /// # Errors
    ///
    /// [`IrRxError::Config`] if the clock is not an exact multiple of 1 MHz up
    /// to 255 MHz, the idle time is 0 or exceeds the hardware limit, or (classic
    /// ESP32, no RX wrap) `CODES` exceeds one RAM block.
    pub fn new(
        channel: impl RxChannelCreator<'d, Blocking>,
        pin: impl PeripheralInput<'d>,
        rmt_clock: Rate,
        config: &IrRxConfig,
    ) -> Result<Self, IrRxError> {
        let divider = tick_divider(rmt_clock).ok_or(IrRxError::Config)?;
        // Zero would end every frame on the first tick.
        if config.idle_us == 0 {
            return Err(IrRxError::Config);
        }
        if !esp_hal::rmt::HAS_RX_WRAP && CODES > esp_hal::rmt::CHANNEL_RAM_SIZE {
            return Err(IrRxError::Config);
        }
        // The hardware glitch filter counts group-clock cycles and tops out near 3 us,
        // so it stays off and `tamer` drops pulses under `min_pulse_us`.
        let cfg = RxChannelConfig::default()
            .with_clk_divider(divider)
            .with_carrier_modulation(false)
            .with_filter_threshold(0)
            .with_idle_threshold(config.idle_us)
            .with_memsize(1);
        let channel = channel
            .configure_rx(&cfg)
            .map_err(|_| IrRxError::Config)?
            .with_pin(pin);
        // The RMT delimits frames, so the assembler's end gap is zero.
        Ok(Self {
            channel,
            assembler: FrameAssembler::new(config.min_pulse_us, 0),
        })
    }

    /// Wait up to `timeout_ms` for one frame.
    ///
    /// Returns `Ok(None)` on timeout (a frame still in flight is dropped), or
    /// the pulse list (microseconds, mark first) borrowed from the receiver
    /// until the next call.
    /// Decode it with [`PulseCodec::decode`].
    /// A repeat code arrives as its own short frame.
    /// A frame in flight at a call boundary can arrive truncated; treat a decode error as droppable.
    ///
    /// # Errors
    ///
    /// [`IrRxError::Overflow`] if the frame exceeds the buffers or the RMT memory,
    /// [`IrRxError::Hardware`] for any other RMT receive error.
    pub fn receive(&mut self, timeout_ms: u32) -> Result<Option<&[u32]>, IrRxError> {
        let mut buf = [PulseCode::default(); CODES];
        let total = {
            let mut transaction = self
                .channel
                .reborrow()
                .receive(&mut buf)
                .map_err(|_| IrRxError::Hardware)?;
            let start = Instant::now();
            let timeout = Duration::from_millis(u64::from(timeout_ms));
            // Dropping the transaction on timeout stops the receiver.
            while !transaction.poll() {
                if start.elapsed() >= timeout {
                    return Ok(None);
                }
            }
            match transaction.wait() {
                Ok((total, _)) => total,
                Err((
                    esp_hal::rmt::Error::Overflow
                    | esp_hal::rmt::Error::InvalidDataLength
                    | esp_hal::rmt::Error::ReceiverError,
                    _,
                )) => return Err(IrRxError::Overflow),
                Err(_) => return Err(IrRxError::Hardware),
            }
        };

        // Without an end marker the RMT stopped on a full buffer (ESP32 has no RX
        // wrap), so the capture is cut short.
        if total > 0 && !buf.iter().take(total).any(|code| code.is_end_marker()) {
            return Err(IrRxError::Overflow);
        }

        // Active-low module: a Low segment is a mark. The assembler owns the
        // timestamping, merging, end-of-frame close and glitch filter.
        let segments = buf.iter().take(total).flat_map(|code| {
            [
                (code.level1() == Level::Low, u32::from(code.length1())),
                (code.level2() == Level::Low, u32::from(code.length2())),
            ]
        });
        match self.assembler.feed_segments(segments) {
            None => Ok(None),
            Some(Ok(frame)) => Ok(Some(frame)),
            Some(Err(_)) => Err(IrRxError::Overflow),
        }
    }
}

impl<const CODES: usize> fmt::Debug for IrTransmitter<'_, CODES> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IrTransmitter")
            .field("codes", &CODES)
            .finish_non_exhaustive()
    }
}

impl<const CODES: usize, const PULSES: usize> fmt::Debug for IrReceiver<'_, CODES, PULSES> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IrReceiver")
            .field("codes", &CODES)
            .field("assembler", &self.assembler)
            .finish_non_exhaustive()
    }
}

impl<const CODES: usize> PulseSink for IrTransmitter<'_, CODES> {
    type Error = IrTxError;

    fn send(&mut self, pulses: &[u32]) -> Result<(), IrTxError> {
        IrTransmitter::send(self, pulses)
    }
}

impl<const CODES: usize, const PULSES: usize> PulseSource for IrReceiver<'_, CODES, PULSES> {
    type Error = IrRxError;

    fn receive(&mut self, timeout_ms: u32) -> Result<Option<&[u32]>, IrRxError> {
        IrReceiver::receive(self, timeout_ms)
    }
}
