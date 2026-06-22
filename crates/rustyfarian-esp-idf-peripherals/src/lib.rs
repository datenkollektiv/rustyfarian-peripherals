//! ESP-IDF (std) hardware tier of the rustyfarian peripherals stack.
//!
//! This crate provides the `std` / ESP-IDF input drivers that bind the pure
//! input logic in [`tamer`] to real ESP32 GPIO via `esp-idf-hal`'s `PinDriver`
//! and interrupt subscriptions — debounced inputs, rotary encoders, and button
//! events for firmware running on the ESP-IDF runtime.
//!
//! Everything public in [`tamer`] is re-exported here, so firmware needs a
//! single import once drivers exist.
//!
//! # Status — skeleton
//!
//! No drivers yet. The crate is a thin re-export of [`tamer`] with the
//! `build.rs` chip-cfg seam in place. Drivers are added **downstream-driven**:
//! when a consumer needs an input on ESP-IDF, wire `esp-idf-hal` (and `embuild`
//! for the link step — see `build.rs`) and implement thin wrappers that
//! delegate all decoding to `tamer`.
//!
//! See `rustyfarian-esp-idf-power` for the established ESP-IDF wrapper pattern
//! (trait-first, hardware lifecycle only, logic delegated to the pure crate).

#[doc(inline)]
pub use tamer::*;
