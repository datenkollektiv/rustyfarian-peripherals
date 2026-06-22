# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning](https://semver.org/) (pre-1.0: minor
bumps may carry breaking changes).

---

## [Unreleased]

### Added
- `tamer::debounce` — `Debouncer` (caller-injected `u64` ticks; a `0` window
  means no debouncing — transitions on the first changed sample), `Edge`,
  `EdgeDetector`, and a `DebouncedInput<P>` adapter (`hal` feature) with a
  `try_from_pin` constructor that seeds state from the live pin. Ported from
  `rustbox-peripherals` — see [ADR-001](docs/adr/001-input-primitives-origin.md).
- `tamer::rotary` — `QuadratureDecoder` (Gray-code decoding, accumulator-based
  detent debouncing), `EncoderDirection`, and a `QuadratureInput<A, B>` adapter
  (`hal` feature) with a `try_from_pins` constructor that seeds state from the
  live pins. Ported from `rustyfarian-knob`'s `zoetrope` (relicensed
  MIT → MIT OR Apache-2.0) — see [ADR-001](docs/adr/001-input-primitives-origin.md).
- `tamer::mock::MockInputPin` (`hal` feature) — settable `InputPin` mock with an
  `Infallible` error, for testing the `hal` adapters and downstream reuse.
- Workspace skeleton: `tamer` (pure `no_std` core, optional `embedded-hal` seam)
  plus thin `rustyfarian-esp-hal-peripherals` (esp-hal, `no_std`) and
  `rustyfarian-esp-idf-peripherals` (ESP-IDF, std) re-export tiers.
- Tooling, CI, docs, and dual MIT/Apache-2.0 licensing.
