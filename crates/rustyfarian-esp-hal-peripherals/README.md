# rustyfarian-esp-hal-peripherals

Bare-metal (**esp-hal**, `no_std`) input drivers for ESP32 — the bare-metal
hardware tier of the [rustyfarian peripherals](../../README.md) stack.

Binds the pure input logic in [`tamer`](../tamer) to real ESP32 GPIO using
`esp-hal`'s pin and interrupt APIs, and re-exports `tamer` so firmware needs a
single import.

## Status

Carries `esp-hal` behind the chip features (`esp32c3` / `esp32c6` / `esp32` /
`esp32s3`) and re-exports `tamer` so firmware needs a single import. The first
example, [`hal_c3_b3f`](examples/hal_c3_b3f.rs), debounces a B3F button on an
ESP32-C3 using the pure `tamer::debounce::EdgeDetector`. Library drivers beyond
the re-export are added downstream-driven. Check it with `just check-hal`; build
or flash examples with `just build-example hal_c3_b3f` / `just run hal_c3_b3f`.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.
