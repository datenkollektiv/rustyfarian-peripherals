//! ESP-IDF (std) hardware tier of the rustyfarian peripherals stack.
//!
//! This crate provides the `std` / ESP-IDF input drivers that bind the pure
//! input logic in [`tamer`] to real ESP32 GPIO via `esp-idf-hal`'s `PinDriver`
//! and interrupt subscriptions — debounced inputs, rotary encoders, and button
//! events for firmware running on the ESP-IDF runtime.
//!
//! Everything public in [`tamer`] is re-exported here, so firmware needs a
//! single import once drivers exist. This crate's own [`rotary`] module
//! shadows the glob-reexported `tamer::rotary` module path (Rust resolves an
//! explicit local item over a glob import unambiguously), so
//! `tamer::rotary::QuadratureDecoder` and `tamer::rotary::EncoderDirection`
//! stay reachable through this crate's flattened top-level re-export
//! (`rustyfarian_esp_idf_peripherals::QuadratureDecoder`,
//! `rustyfarian_esp_idf_peripherals::EncoderDirection`) rather than through
//! `rustyfarian_esp_idf_peripherals::rotary::`.
//!
//! # Interrupt-driven GPIO on this tier
//!
//! Two distinct patterns show up for interrupt-driven input on `esp-idf-hal`,
//! and this crate picks between them per driver rather than standardizing on
//! one:
//!
//! - **Polled one-shot HAL subscriptions** (`PinDriver::subscribe` +
//!   `enable_interrupt`): the HAL's built-in async-notification pattern. The
//!   interrupt auto-disables after the first fire and must be explicitly
//!   re-armed, which suits a low-frequency, one-edge-at-a-time signal (e.g.
//!   waking a task on a button press) but cannot keep up with a dense,
//!   continuous edge stream.
//! - **Persistent raw-FFI interrupts** (`gpio_isr_handler_add` via
//!   `esp_idf_svc::sys`, bypassing the HAL's subscription API): registers a
//!   handler that stays armed after every edge. Required for edge-dense
//!   inputs such as quadrature rotary encoders, where a slow main-loop tick
//!   (e.g. a long display DMA transfer) could otherwise miss edges between
//!   polls. See [`rotary::Encoder`] for the reference implementation and its
//!   module docs for the full rationale, including the load-bearing
//!   ISR-teardown ordering in its `Drop` impl.
//!
//! Default to the polled `tick`/sample style (feeding a pure `tamer` state
//! machine from a fixed-cadence loop) unless a downstream project has an
//! established, hardware-verified need for zero event loss under load — that
//! is a deliberately high bar for taking on raw-FFI interrupt plumbing.
//!
//! # Status
//!
//! The first driver has landed: [`rotary::Encoder`], an interrupt-driven
//! quadrature rotary encoder with a debounced push button. Further drivers
//! are added **downstream-driven**: when a consumer needs an input on
//! ESP-IDF, implement a thin wrapper that delegates all decoding to `tamer`.
//!
//! See `rustyfarian-esp-idf-power` for the established ESP-IDF wrapper pattern
//! (trait-first, hardware lifecycle only, logic delegated to the pure crate).

pub mod rotary;

#[doc(inline)]
pub use tamer::*;
