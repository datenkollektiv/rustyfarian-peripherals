# rustyfarian-esp-hal-peripherals

Bare-metal (**esp-hal**, `no_std`) input drivers for ESP32 — the bare-metal
hardware tier of the [rustyfarian peripherals](../../README.md) stack.

Binds the pure input logic in [`tamer`](../tamer) to real ESP32 GPIO using
`esp-hal`'s pin and interrupt APIs, and re-exports `tamer` so firmware needs a
single import.

## Status

Skeleton — a thin re-export of `tamer` with the chip-feature
(`esp32c3` / `esp32c6` / `esp32` / `esp32s3`) and `build.rs` cfg seams in place.
Drivers are added downstream-driven; the `esp-hal` dependency is wired behind
the chip features when the first driver lands. See
`rustyfarian-esp-hal-network` for the esp-hal feature-gating pattern to follow.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.
