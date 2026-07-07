#![cfg_attr(not(test), no_std)]
//! `tamer` — taming unruly hardware inputs into clean, testable events.
//!
//! `tamer` is the pure, host-buildable core of the rustyfarian *peripherals*
//! stack. It holds the input logic that has no business touching hardware —
//! debounce state machines, rotary-encoder quadrature decoding, and
//! higher-level button events (press, release, long-press, double-click). See
//! the [Status](#status) section for what has landed.
//!
//! The physical world is noisy. Mechanical buttons bounce; rotary encoders emit
//! ragged quadrature transitions; lines float when nothing drives them. `tamer`
//! is the act that turns that mess into a calm, predictable stream of events —
//! and, because none of that logic needs a chip, all of it can be unit-tested on
//! your laptop without an ESP32 or an ESP toolchain.
//!
//! # Rustyfarian philosophy
//!
//! This crate embodies the family principle of **extracting testable pure logic
//! from hardware-specific code** — common in application development, rare in
//! embedded Rust:
//!
//! - **Pure logic lives here.** Decoding and timing state machines are plain
//!   Rust with no hardware dependency, so they are fully host-testable.
//! - **Trait-first.** Every hardware interaction is expressed behind a trait;
//!   consumers program against the trait, not a concrete pin type.
//! - **A `Noop*` mock ships with every trait.** Downstream test suites use the
//!   crate's own mocks rather than inventing their own.
//! - **The `hal` feature is the only hardware seam.** Enabling it adds thin
//!   adapters over [`embedded_hal::digital::InputPin`] that feed the pure logic;
//!   the default build pulls in nothing hardware-related.
//!
//! The thin, chip-specific glue lives in the companion hardware crates
//! (`rustyfarian-esp-hal-peripherals` for bare-metal esp-hal, and
//! `rustyfarian-esp-idf-peripherals` for ESP-IDF / std), which re-export this
//! crate so firmware needs a single import.
//!
//! # Status
//!
//! The following primitives have landed:
//!
//! - [`analog`] — raw ADC range normalization and deadbanded analog movement.
//! - [`debounce`] — sampled-input debounce state machine and edge detector.
//! - [`hall`] — Hall-effect magnetic presence detection via configurable
//!   deviation threshold; calibrates the no-magnet midpoint from samples.
//! - [`smoothing`] — fixed-size O(1) sliding-window average for absorbing ADC
//!   quantization noise before threshold evaluation.
//! - [`presence`] — polarity-aware debounced present / absent detection for
//!   digital sensors.
//! - [`rotary`] — quadrature / Gray-code decoding with detent handling.
//! - [`button`] — press / release / click / double-click / long-press events,
//!   built on the [`debounce`] edge detector.
//!
//! Still pending (arrive on demand, driven by real downstream needs):
//!
//! - `touch` — capacitive touch event detection.
//! - `display` — simple character display abstractions.

/// Debounced digital input — [`Debouncer`](debounce::Debouncer),
/// [`Edge`](debounce::Edge), and [`EdgeDetector`](debounce::EdgeDetector).
///
/// Enable the `hal` feature to get the
/// [`DebouncedInput`](debounce::DebouncedInput) adapter that reads an
/// `embedded-hal` `InputPin` directly.
pub mod debounce;
pub use debounce::{Debouncer, Edge, EdgeDetector};

#[cfg(feature = "hal")]
pub use debounce::DebouncedInput;

/// Analog input helpers — [`AnalogCalibration`](analog::AnalogCalibration),
/// [`AnalogRange`](analog::AnalogRange), [`AnalogValue`](analog::AnalogValue), and
/// [`AnalogInput`](analog::AnalogInput).
///
/// This module is HAL-agnostic.
/// Hardware tiers feed it raw ADC samples; host tests can use
/// [`MockAnalogRead`](analog::MockAnalogRead).
pub mod analog;
pub use analog::{
    AnalogCalibration, AnalogInput, AnalogRange, AnalogRead, AnalogSample, AnalogValue,
    MockAnalogRead,
};

/// Hall-effect magnetic presence detection — [`HallSensor`](hall::HallSensor)
/// and [`HallCalibrationError`](hall::HallCalibrationError).
///
/// Pure, HAL-agnostic threshold evaluator: feed raw ADC samples, calibrate
/// the no-magnet midpoint with
/// [`calibrate_from_samples`](hall::HallSensor::calibrate_from_samples), and
/// call [`evaluate`](hall::HallSensor::evaluate) to obtain a
/// [`Presence`](presence::Presence) reading.
pub mod hall;
pub use hall::{HallCalibrationError, HallSensor};

/// Sliding-window average smoother — [`SlidingAverage`](smoothing::SlidingAverage).
///
/// Maintains a circular `[u16; N]` buffer with a running `u32` sum so each
/// `push` is O(1).
/// Compose with [`hall`] or any threshold-based evaluator to absorb ADC
/// quantization noise before detection.
pub mod smoothing;
pub use smoothing::SlidingAverage;

/// Digital presence detection — [`Presence`](presence::Presence),
/// [`Polarity`](presence::Polarity), and
/// [`DigitalPresence`](presence::DigitalPresence).
///
/// Enable the `hal` feature to get the
/// [`DigitalPresenceInput`](presence::DigitalPresenceInput) adapter that reads
/// an `embedded-hal` `InputPin` directly.
pub mod presence;
pub use presence::{DigitalPresence, Polarity, Presence};

#[cfg(feature = "hal")]
pub use presence::DigitalPresenceInput;

/// Quadrature rotary encoder decoder — [`QuadratureDecoder`](rotary::QuadratureDecoder)
/// and [`EncoderDirection`](rotary::EncoderDirection).
///
/// Enable the `hal` feature to get the
/// [`QuadratureInput`](rotary::QuadratureInput) adapter that reads two
/// `embedded-hal` `InputPin`s directly.
pub mod rotary;
pub use rotary::{EncoderDirection, QuadratureDecoder};

#[cfg(feature = "hal")]
pub use rotary::QuadratureInput;

/// Button-event decoder — [`ButtonDecoder`](button::ButtonDecoder) and
/// [`ButtonEvent`](button::ButtonEvent). Debounces a press/release signal (via
/// the [`debounce`] edge detector) and emits press, release, click,
/// double-click, and long-press events.
///
/// Enable the `hal` feature to get the [`ButtonInput`](button::ButtonInput)
/// adapter that reads an `embedded-hal` `InputPin` (active-low or active-high)
/// directly.
pub mod button;
pub use button::{ButtonDecoder, ButtonEvent};

#[cfg(feature = "hal")]
pub use button::ButtonInput;

/// Settable mock pin for unit-testing the `hal` adapters.
///
/// [`MockInputPin`](mock::MockInputPin) implements
/// `embedded_hal::digital::InputPin` with an `Infallible` error type.
/// Downstream crates should use it as a drop-in for real GPIO in host tests.
#[cfg(feature = "hal")]
pub mod mock;

#[cfg(feature = "hal")]
pub use mock::MockInputPin;

/// Curated re-exports of the most-used types, for `use tamer::prelude::*;`.
///
/// Covers the pure types unconditionally and the `hal` adapters when the
/// `hal` feature is enabled.
pub mod prelude {
    pub use crate::analog::{
        AnalogCalibration, AnalogInput, AnalogRange, AnalogRead, AnalogSample, AnalogValue,
        MockAnalogRead,
    };
    pub use crate::button::{ButtonDecoder, ButtonEvent};
    pub use crate::debounce::{Debouncer, Edge, EdgeDetector};
    pub use crate::hall::{HallCalibrationError, HallSensor};
    pub use crate::presence::{DigitalPresence, Polarity, Presence};
    pub use crate::rotary::{EncoderDirection, QuadratureDecoder};
    pub use crate::smoothing::SlidingAverage;

    #[cfg(feature = "hal")]
    pub use crate::button::ButtonInput;
    #[cfg(feature = "hal")]
    pub use crate::debounce::DebouncedInput;
    #[cfg(feature = "hal")]
    pub use crate::mock::MockInputPin;
    #[cfg(feature = "hal")]
    pub use crate::presence::DigitalPresenceInput;
    #[cfg(feature = "hal")]
    pub use crate::rotary::QuadratureInput;
}
