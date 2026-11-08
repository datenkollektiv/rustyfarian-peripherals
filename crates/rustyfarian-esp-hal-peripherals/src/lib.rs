#![no_std]
//! Bare-metal (esp-hal) hardware tier of the rustyfarian peripherals stack.
//!
//! This crate provides the `no_std`, bare-metal esp-hal drivers that bind the
//! pure input logic in [`tamer`] to real ESP32 GPIO — debounced inputs, rotary
//! encoders, and button events on top of `esp-hal`'s pin and interrupt APIs.
//!
//! Everything public in [`tamer`] is re-exported here, so firmware needs a
//! single import.
//! This crate's own [`rotary`] and [`analog`] modules shadow the
//! glob-reexported `tamer::rotary` / `tamer::analog` module paths (an explicit
//! local item wins over a glob import); each local module re-exports the
//! matching `tamer` items, and the flattened top-level re-export stays intact.
//! [`analog::AdcReader`] is the one-line raw-count ADC1 reader.
//!
//! With a `board-*` feature, the [`board`] module names the board's I2C pins
//! and bus power (ADR-014).
//!
//! # Status — first driver
//!
//! The first driver is the async rotary [`Encoder`](rotary::Encoder) in
//! [`rotary`], the esp-hal twin of the ESP-IDF tier's encoder. It is a thin
//! wrapper that delegates all decoding to `tamer`.
//! The sync [`InterruptEncoder`](rotary::InterruptEncoder) implements
//! [`tamer::rotary::RotaryEncoder`]; the async `Encoder` deliberately does not.
//! The chip feature
//! (`esp32c3` / `esp32c6` / `esp32` / `esp32s3`) and `build.rs` cfg seams are in
//! place. Further drivers are added **downstream-driven**: when a consumer
//! needs one, it is implemented as a thin wrapper that delegates all decoding
//! to `tamer`.
//!
//! See `rustyfarian-network`'s `rustyfarian-esp-hal-network` crate for the
//! established esp-hal feature-gating and async patterns to follow.

pub mod analog;
#[cfg(any(
    feature = "board-c3-supermini",
    feature = "board-c6-devkitc",
    feature = "board-esp32-wroom",
    feature = "board-feather-v2"
))]
pub mod board;
pub mod rotary;

// Compile-time guard: fails to build if the concrete type stops implementing
// the shared trait. Zero cost; generates no code.
const _: () = {
    const fn assert_impl<E: tamer::rotary::RotaryEncoder>() {}
    assert_impl::<rotary::InterruptEncoder<'static>>();
};

// Conformance guard: pins what the `RotaryEncoder` trait does not enforce.
// The inherent diagnostics counters keep identical signatures on both tiers,
// and no local item shadows the glob-re-exported tamer `RotaryEncoder` /
// `NoopRotaryEncoder`. Zero cost; never called.
const _: fn() = || {
    type Tier = rotary::InterruptEncoder<'static>;
    let _: fn(&Tier) -> u32 = Tier::edge_count;
    let _: fn(&Tier) -> u32 = Tier::skipped_count;
    let _: core::marker::PhantomData<tamer::rotary::NoopRotaryEncoder> =
        core::marker::PhantomData::<crate::NoopRotaryEncoder>;
    fn same_trait<T: crate::RotaryEncoder>() {}
    same_trait::<tamer::rotary::NoopRotaryEncoder>();
};

// Conformance guard: `analog::AdcReader` (concrete ADC1 pin) implements the
// shared `AnalogRead` trait, and `analog::MockAnalogRead` is the tamer item
// despite the local `analog` module shadowing the glob re-export.
const _: fn() = || {
    fn assert_impl<R: tamer::analog::AnalogRead>() {}
    #[cfg(esp32)]
    assert_impl::<analog::AdcReader<'static, esp_hal::peripherals::GPIO32<'static>>>();
    #[cfg(not(esp32))]
    assert_impl::<analog::AdcReader<'static, esp_hal::peripherals::GPIO4<'static>>>();
    let _: core::marker::PhantomData<tamer::analog::MockAnalogRead> =
        core::marker::PhantomData::<analog::MockAnalogRead>;
};

#[doc(inline)]
pub use tamer::*;
