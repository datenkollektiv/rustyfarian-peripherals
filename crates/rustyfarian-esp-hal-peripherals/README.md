# rustyfarian-esp-hal-peripherals

Bare-metal (**esp-hal**, `no_std`) input drivers for ESP32 — the bare-metal
hardware tier of the [rustyfarian peripherals](../../README.md) stack.

Binds the pure input logic in [`tamer`](../tamer) to real ESP32 GPIO using
`esp-hal`'s pin and interrupt APIs, and re-exports `tamer` so firmware needs a
single import.

## Status

Carries `esp-hal` behind the chip features (`esp32c3` / `esp32c6` / `esp32` /
`esp32s3`) and re-exports `tamer` so firmware needs a single import. Ships
`rustyfarian_esp_hal_peripherals::rotary::Encoder`, an async quadrature decoder
that awaits edges via `Input::wait_for_any_edge` and feeds `tamer`'s pure
`QuadratureDecoder` and `ButtonDecoder`. The example [`hal_s3_rotary`](examples/hal_s3_rotary.rs)
demonstrates the CrowPanel 1.28" ESP32-S3 knob; build it with
`just build-example hal_s3_rotary` (requires the `embassy` feature, which the
example's `required-features` select).

Contract: rotation is awaited asynchronously, button gestures are polled
synchronously, and all decoding lives in `tamer`.
The `Encoder` itself is executor-agnostic and depends only on `esp-hal`; the
`embassy` feature (`esp-rtos` + Embassy) exists only for the example runtime.
Firmware without an async executor should use `tamer`'s polled adapters
(`tamer::rotary::QuadratureInput`, `tamer::button::ButtonDecoder`) instead.
 Earlier examples like [`hal_c3_b3f`](examples/hal_c3_b3f.rs)
still debounce a B3F button on ESP32-C3 using `tamer::debounce::EdgeDetector`.
Check it all with `just check-hal`; hardware examples run with `just run`.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.
