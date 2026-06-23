# rustyfarian-esp-idf-peripherals

ESP-IDF (**std**) input drivers for ESP32 — the std hardware tier of the
[rustyfarian peripherals](../../README.md) stack.

Binds the pure input logic in [`tamer`](../tamer) to real ESP32 GPIO via
`esp-idf-hal` (`PinDriver`, interrupt subscriptions), and re-exports `tamer` so
firmware needs a single import.

## Status

Carries `esp-idf-hal` (with `embuild` re-emitting the ESP-IDF link step in
`build.rs`) and re-exports `tamer` so firmware needs a single import. The first
example, [`idf_c3_b3f`](examples/idf_c3_b3f.rs), debounces a B3F button on an
ESP32-C3 using the pure `tamer::debounce::EdgeDetector`. Library drivers beyond
the re-export are added downstream-driven. Build or flash examples with
`just build-example idf_c3_b3f` / `just run idf_c3_b3f` (needs the ESP toolchain).

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.
