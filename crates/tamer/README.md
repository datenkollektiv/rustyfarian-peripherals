# tamer

Platform-agnostic, host-testable **input primitives** for embedded projects —
the pure core of the [rustyfarian peripherals](../../README.md) stack.

`tamer` tames unruly hardware inputs (bouncing buttons, noisy presence sensors,
rotary encoders, floating lines, and ADC-backed controls) into a calm,
predictable stream of events.
All of its logic is plain `no_std` Rust with no hardware dependency, so it is
fully unit-testable on the host — no ESP32, no ESP toolchain.

## Design

- **Pure logic only.** Debounce, digital presence detection, rotary quadrature
  decoding, button-event timing, and analog normalization are state machines
  with no hardware dependency.
- **Trait-first**, with a `Noop*` mock shipped beside every trait for downstream
  testing.
- **`hal` feature** (optional): thin adapters over
  `embedded_hal::digital::InputPin` that feed the pure logic. Off by default, so
  the core stays hardware-free.

The chip-specific drivers live in the companion crates
`rustyfarian-esp-hal-peripherals` (bare-metal) and
`rustyfarian-esp-idf-peripherals` (ESP-IDF / std).

## Status

Primitives are added **on demand, driven by real downstream needs** — not
speculatively.
See the crate-level docs (`src/lib.rs`) for the primitives that have landed.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.
