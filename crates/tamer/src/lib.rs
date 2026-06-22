#![cfg_attr(not(test), no_std)]
//! `tamer` — taming unruly hardware inputs into clean, testable events.
//!
//! `tamer` is the pure, host-buildable core of the rustyfarian *peripherals*
//! stack. It holds the input logic that has no business touching hardware:
//! debounce state machines, rotary-encoder quadrature decoding, and
//! button-event detection (press, release, long-press, double-click).
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
//! # Status — skeleton
//!
//! This crate is intentionally empty of primitives today. The peripherals stack
//! grows **on demand, driven by real downstream needs** (the same demand-driven
//! discipline as the sibling rustyfarian crates), not speculatively. Each
//! primitive lands in its own module following the pattern above:
//!
//! - `debounce` — a sampled-input debounce state machine
//! - `rotary` — quadrature / Gray-code decoding with detent handling
//! - `button` — higher-level press / long-press / double-click events
//!
//! When the first consumer needs one of these, implement it here behind a
//! trait, ship its `Noop*` mock in the same change, add host tests, and (if it
//! has a hardware adapter) wire it behind the `hal` feature. Then surface the
//! public types through [`prelude`].

// Input primitives are added on demand — see "Status" above. Keep this module
// list and the `prelude` re-exports in lockstep as they land.

/// Curated re-exports of the most-used types, for `use tamer::prelude::*;`.
///
/// Empty while the crate is a skeleton; populated as primitives land so that
/// downstream firmware has one stable, ergonomic import path.
pub mod prelude {}
