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
//! - [`debounce`] — sampled-input debounce state machine and edge detector.
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
    pub use crate::button::{ButtonDecoder, ButtonEvent};
    pub use crate::debounce::{Debouncer, Edge, EdgeDetector};
    pub use crate::rotary::{EncoderDirection, QuadratureDecoder};

    #[cfg(feature = "hal")]
    pub use crate::button::ButtonInput;
    #[cfg(feature = "hal")]
    pub use crate::debounce::DebouncedInput;
    #[cfg(feature = "hal")]
    pub use crate::mock::MockInputPin;
    #[cfg(feature = "hal")]
    pub use crate::rotary::QuadratureInput;
}
