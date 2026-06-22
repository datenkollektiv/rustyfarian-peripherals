#![no_std]
//! Bare-metal (esp-hal) hardware tier of the rustyfarian peripherals stack.
//!
//! This crate provides the `no_std`, bare-metal esp-hal drivers that bind the
//! pure input logic in [`tamer`] to real ESP32 GPIO — debounced inputs, rotary
//! encoders, and button events on top of `esp-hal`'s pin and interrupt APIs.
//!
//! Everything public in [`tamer`] is re-exported here, so firmware needs a
//! single import once drivers exist.
//!
//! # Status — skeleton
//!
//! No drivers yet. The crate is a thin re-export of [`tamer`] with the chip
//! feature (`esp32c3` / `esp32c6` / `esp32` / `esp32s3`) and `build.rs` cfg
//! seams already in place. Drivers are added **downstream-driven**: when a
//! consumer needs, say, a rotary encoder on an ESP32-C6, the `esp-hal`
//! dependency is wired behind the chip features (see `Cargo.toml`), and the
//! driver is implemented as a thin wrapper that delegates all decoding to
//! `tamer`.
//!
//! See `rustyfarian-network`'s `rustyfarian-esp-hal-network` crate for the
//! established esp-hal feature-gating and async patterns to follow.

#[doc(inline)]
pub use tamer::*;
