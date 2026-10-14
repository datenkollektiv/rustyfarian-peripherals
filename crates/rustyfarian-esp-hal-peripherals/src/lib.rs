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
//! # Status — first driver
//!
//! The first driver is the async rotary [`Encoder`](rotary::Encoder) in
//! [`rotary`], the esp-hal twin of the ESP-IDF tier's encoder. It is a thin
//! wrapper that delegates all decoding to `tamer`. The chip feature
//! (`esp32c3` / `esp32c6` / `esp32` / `esp32s3`) and `build.rs` cfg seams are in
//! place. Further drivers are added **downstream-driven**: when a consumer
//! needs one, it is implemented as a thin wrapper that delegates all decoding
//! to `tamer`.
//!
//! See `rustyfarian-network`'s `rustyfarian-esp-hal-network` crate for the
//! established esp-hal feature-gating and async patterns to follow.

pub mod rotary;

#[doc(inline)]
pub use tamer::*;
