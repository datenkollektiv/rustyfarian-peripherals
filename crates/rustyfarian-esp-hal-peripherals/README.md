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

It also ships `rustyfarian_esp_hal_peripherals::rotary::InterruptEncoder`, a
synchronous quadrature decoder that runs in a GPIO interrupt and keeps
`position()` current while the application does blocking work (for example a
~90 ms display redraw).
Its method surface matches the ESP-IDF tier's `Encoder`, so a consumer switches
tiers by changing the import and constructor.
The application installs the single esp-hal GPIO handler and calls
`rotary::on_gpio_interrupt()` from it; the [`hal_s3_rotary_isr`](examples/hal_s3_rotary_isr.rs)
example shows the wiring, and the `rotary` module docs state the handler
requirements.
Build it with `just build-example hal_s3_rotary_isr`.

Contract: `Encoder` awaits rotation asynchronously, `InterruptEncoder` decodes
rotation in the GPIO interrupt, button gestures are polled synchronously, and
all decoding lives in `tamer`.
The `Encoder` itself is executor-agnostic and depends only on `esp-hal`; the
`embassy` feature (`esp-rtos` + Embassy) exists only for the example runtime.
Firmware without an async executor can use `InterruptEncoder`, or `tamer`'s
polled adapters (`tamer::rotary::QuadratureInput`, `tamer::button::ButtonDecoder`).
 Earlier examples like [`hal_c3_b3f`](examples/hal_c3_b3f.rs)
still debounce a B3F button on ESP32-C3 using `tamer::debounce::EdgeDetector`.
Check it all with `just check-hal`; hardware examples run with `just run`.

## License

Licensed under either of [Apache-2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at
your option.
