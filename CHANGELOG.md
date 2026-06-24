# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning](https://semver.org/) (pre-1.0: minor
bumps may carry breaking changes).

---

## [Unreleased]

### Added
- `tamer` workspace skeleton: the pure `no_std` core plus thin
  `rustyfarian-esp-hal-peripherals` (esp-hal) and `rustyfarian-esp-idf-peripherals`
  (ESP-IDF) re-export tiers, an optional `embedded-hal` `hal` seam, tooling, CI,
  and dual MIT/Apache-2.0 licensing.
- `tamer::debounce` — `Debouncer`, `Edge`, `EdgeDetector`, and the `hal`-gated
  `DebouncedInput<P>` adapter (caller-owned `u64` clock; `try_from_pin`).
- `tamer::presence` — `Presence`, `Polarity`, `DigitalPresence`, and the
  `hal`-gated `DigitalPresenceInput<P>` adapter for polarity-aware debounced
  digital presence detection.
- `tamer::rotary` — `QuadratureDecoder`, `EncoderDirection`, and the `hal`-gated
  `QuadratureInput<A, B>` adapter (`try_from_pins`).
- `tamer::button` — `ButtonDecoder` and `ButtonEvent` (raw `Press`/`Release`
  edges plus layered `Click`/`DoubleClick`/`LongPress` gestures), and the
  `hal`-gated `ButtonInput<P>` adapter (active-low/high; `try_from_pin`).
- `tamer::mock::MockInputPin` (`hal`) — settable `InputPin` mock for host tests.
- ESP32-C3 B3F button examples on both esp tiers (`hal_c3_b3f`, `idf_c3_b3f`),
  wiring the first hardware dependency (`esp-hal` / `esp-idf-hal`) and the
  `build-example` / `run` / `check-hal` justfile recipes.
- Primitives donated by clean reimplementation (relicensed to MIT OR Apache-2.0)
  from `rustyfarian-knob` and `rustbox-peripherals`; the button event contract
  intentionally diverges from the knob's, and digital presence follows the
  donor repo's accepted abstraction boundary — see
  [ADR-001](docs/adr/001-input-primitives-origin.md) and
  [ADR-002](docs/adr/002-digital-presence.md).
