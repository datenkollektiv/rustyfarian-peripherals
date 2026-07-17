#![cfg_attr(not(test), no_std)]
//! `tamer` — taming unruly hardware inputs and sequencing outputs into clean, testable logic.
//!
//! `tamer` is the pure, host-buildable core of the rustyfarian *peripherals*
//! stack. It holds the logic that has no business touching hardware —
//! debounce state machines, rotary-encoder quadrature decoding, button events
//! (press, release, long-press, double-click), and tone/duration sequencing.
//! See the [Status](#status) section for what has landed.
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
//!   crate's own mocks rather than inventing their own. Seam-free pure modules
//!   ([`hall`], [`mpu6050`], [`touch`]) have no trait to mock — the raw-sample
//!   feed itself is the test seam.
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
//! - [`smoothing`] — fixed-size O(1) sliding-window average and an
//!   exponential moving average ([`EmaFilter`]) for absorbing ADC
//!   quantization noise before threshold evaluation.
//! - [`presence`] — polarity-aware debounced present / absent detection for
//!   digital sensors.
//! - [`range_map`] — clamped linear remap from a `u16` analog reading to a
//!   `u8` output (e.g. ADC counts to LEDC PWM duty).
//! - [`rotary`] — quadrature / Gray-code decoding with detent handling.
//! - [`button`] — press / release / click / double-click / long-press events,
//!   built on the [`debounce`] edge detector.
//! - [`mpu6050`] — MPU6050 IMU register map, raw-burst parsing, and
//!   accelerometer offset calibration; `tamer`'s first device-named module.
//!   Pair with the `tilt` feature for tilt-angle trigonometry.
//! - [`tone`] — a tone/duration sequencer (melody player) stepping a borrowed
//!   `&[Note]` table into [`ToneOutput`] values;
//!   `tamer`'s first output/actuator primitive, feeding a buzzer/PWM/DAC
//!   adapter downstream.
//! - [`touch`] — touch event detection: raw `Down`/`Move`/`Up` contact edges
//!   plus derived `Tap`/`LongPress`/`Swipe` gestures from per-frame
//!   `Option<TouchPoint>` samples; chip-agnostic, works on controllers
//!   without a hardware gesture engine.
//!
//! Still pending (arrive on demand, driven by real downstream needs):
//!
//! - `display` — pure, host-testable UI *logic* above a `DrawTarget`, composing
//!   with [`touch`]: touch-region hit-testing first, then framebuffer dirty-rect
//!   diffing. `embedded-graphics` stays the display HAL — `tamer` grows no
//!   display trait and no `embedded-graphics` dependency, and text / layout are
//!   reused upstream. See ADR-008 for the full rationale.

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

/// Sliding-window average smoother — [`SlidingAverage`](smoothing::SlidingAverage) —
/// and exponential moving average filter — [`EmaFilter`](smoothing::EmaFilter).
///
/// `SlidingAverage` maintains a circular `[u16; N]` buffer with a running
/// `u32` sum so each `push` is O(1).
/// `EmaFilter` is its `f32`, exponentially-weighted sibling: a single
/// accumulator updated by `output = alpha * input + (1 - alpha) * previous`.
/// Compose with [`hall`] or any threshold-based evaluator to absorb ADC
/// quantization noise before detection.
pub mod smoothing;
pub use smoothing::{EmaFilter, SlidingAverage};

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

/// Clamped linear remap from a `u16` analog reading to a `u8` output —
/// [`RangeMap`](range_map::RangeMap).
///
/// This module is HAL-agnostic and imports nothing outside `tamer`.
/// Pair it with [`analog`] (and [`smoothing`] for noisy sources) to turn a
/// raw ADC reading into a PWM duty or similar `u8` output.
pub mod range_map;
pub use range_map::RangeMap;

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

/// MPU6050 IMU protocol constants, raw-burst parsing, and accelerometer
/// calibration — [`RawReading`](mpu6050::RawReading),
/// [`parse_raw`](mpu6050::parse_raw), [`AccelCalibration`](mpu6050::AccelCalibration),
/// [`AccelOffsets`](mpu6050::AccelOffsets), and [`apply_offsets`](mpu6050::apply_offsets).
///
/// This module is HAL-agnostic and imports no I2C, HAL, or chip crate — the
/// caller performs the I2C burst read and feeds the 14-byte buffer in. Enable
/// the `tilt` feature for `tamer::tilt`'s `atan2`-based tilt-angle
/// trigonometry on the parsed axes.
pub mod mpu6050;
pub use mpu6050::{apply_offsets, parse_raw, AccelCalibration, AccelOffsets, RawReading};

/// Scale-free two-axis tilt-angle trigonometry —
/// [`tilt_degrees`](tilt::tilt_degrees) and [`tilt_degrees_i32`](tilt::tilt_degrees_i32).
///
/// Gated behind the `tilt` feature because `atan2` needs [`micromath`], a
/// `no_std` CORDIC approximation library — `tamer`'s only floating-point
/// trigonometry dependency. Pair with [`mpu6050`] or any other accelerometer
/// source.
#[cfg(feature = "tilt")]
pub mod tilt;

#[cfg(feature = "tilt")]
pub use tilt::{tilt_degrees, tilt_degrees_i32};

/// Tone/duration sequencer (melody player) — [`ToneSequencer`], [`Note`],
/// [`SequenceMode`], [`ToneOutput`], and [`SequenceEvent`].
///
/// This module is HAL-agnostic and imports nothing outside `tamer`; it never
/// touches a pin, PWM peripheral, or DAC. `tamer`'s first output/actuator
/// primitive — a downstream hardware adapter drives a buzzer or speaker from
/// the [`ToneOutput`] values.
pub mod tone;
pub use tone::{Note, SequenceEvent, SequenceMode, ToneOutput, ToneSequencer};

/// Touch-panel event detection — [`TouchTracker`](touch::TouchTracker),
/// [`TouchEvent`](touch::TouchEvent), [`TouchPoint`](touch::TouchPoint), and
/// [`SwipeDirection`](touch::SwipeDirection).
///
/// This module is HAL-agnostic and imports nothing outside `tamer` — no
/// `embedded-hal` touch trait exists, so the chip driver feeds decoded,
/// calibrated `Option<TouchPoint>` frames straight into the pure tracker.
/// Resistive controllers debounce the raw touched flag upstream with a
/// [`Debouncer`](debounce::Debouncer).
pub mod touch;
pub use touch::{SwipeDirection, TouchEvent, TouchPoint, TouchTracker};

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
    pub use crate::mpu6050::{AccelCalibration, RawReading};
    pub use crate::presence::{DigitalPresence, Polarity, Presence};
    pub use crate::range_map::RangeMap;
    pub use crate::rotary::{EncoderDirection, QuadratureDecoder};
    pub use crate::smoothing::{EmaFilter, SlidingAverage};
    pub use crate::tone::{Note, SequenceMode, ToneSequencer};
    pub use crate::touch::{SwipeDirection, TouchEvent, TouchPoint, TouchTracker};

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
