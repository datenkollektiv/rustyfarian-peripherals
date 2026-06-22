# rustyfarian-esp-idf-peripherals

ESP-IDF (**std**) input drivers for ESP32 — the std hardware tier of the
[rustyfarian peripherals](../../README.md) stack.

Binds the pure input logic in [`tamer`](../tamer) to real ESP32 GPIO via
`esp-idf-hal` (`PinDriver`, interrupt subscriptions), and re-exports `tamer` so
firmware needs a single import.

## Status

Skeleton — a thin re-export of `tamer` with the `build.rs` chip-cfg seam in
place. Drivers are added downstream-driven; `esp-idf-hal` and `embuild` are
wired when the first driver lands (see `build.rs` for the required
`embuild::espidf::sysenv::output()` link step). See `rustyfarian-esp-idf-power`
for the ESP-IDF wrapper pattern to follow.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.
