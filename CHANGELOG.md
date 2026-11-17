# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning](https://semver.org/) (pre-1.0: minor bumps may carry breaking changes).

---

## [Unreleased]

### Added
- `tamer::ir` (opt-in `ir` feature, no dependency): an infrared pulse codec whose timings are data.
- `PulseCodec` (header, zero/one bit timings, trailer, 1..=32 bits, bit order, `Tolerance`) offers `encode_into`/`encode` and a tolerant `decode` that accepts a truncated first header mark.
- `PulseCodec::nec()`, `NecFrame` (address/command packing with inverted-byte check, extended 16-bit address) and `NEC_REPEAT`/`is_nec_repeat` cover standard NEC.
- `FrameAssembler` turns edge timestamps into a pulse list with a glitch filter and an end-of-frame gap, in the caller-driven `update(now)` style.
- `PulseBuf` and `pulse_pairs` carry pulse lists across to a hardware tier.
- `FrameAssembler::feed_segments` assembles a delimited capture (such as RMT entries) in one call, and `validate_pulses` checks a pulse list for a transmitter; both keep that logic host-tested.
- `PulseSink` / `PulseSource` traits abstract the transmit and receive bridges, with `MockPulseSink` / `MockPulseSource` for host tests; `FrameAssembler` derives `Clone` and `Debug`.
- Only `PulseCodec`, `NecFrame`, `PulseBuf`, `FrameAssembler`, the two traits and the mocks are re-exported at the crate root; `Tolerance`, `BitOrder`, the error types, `carrier_ticks`, `pulse_pairs`, `validate_pulses`, `is_nec_repeat` and `NEC_REPEAT` stay under `tamer::ir::`.
- `ir::carrier_ticks` converts a carrier frequency and duty into high/low clock ticks for a hardware modulator (`(694, 1411)` for 38 kHz / 33 % at 80 MHz).
- `rustyfarian-esp-hal-peripherals` (opt-in `ir` feature): `ir::IrTransmitter` and `ir::IrReceiver` bridge `tamer::ir` to the RMT peripheral.
- `IrTransmitter::send` modulates a pulse list onto a carrier (default 38 kHz / 33 %) on any RMT TX channel.
- `IrReceiver::receive` captures one frame from an active-low demodulated module, delimited by the RMT idle threshold (default 20 ms), and returns the pulse list for `PulseCodec::decode`.
- Examples `hal_c3_ir_tx` (GPIO 10, NEC and an 8-bit candle code) and `hal_c3_ir_rx` (GPIO 4).
- The bridge is compile-verified on all four chips; carrier timing is source-derived and needs a scope or TSOP check on hardware.
- `tamer::staged`: `StagedTimer` steps a borrowed `&[Stage]` table of `count` steps of `interval_ticks` each on a caller-driven tick clock.
- It reports `StagedEvent::Step` when a step is due (the first at the start tick) and `StagedEvent::Finished` once at the end.
- `StagedTimer::new(stages, StagedMode)` takes `StagedMode::Once` or `StagedMode::Loop`; `Loop` wraps to the first non-empty stage and never finishes, for endless props.
- `StagedTimer::stage()` returns the table index of the stage the latest `Step` belongs to, so a caller can single out a final hold stage.
- An empty or all-zero table reports `Finished` on the first `update`, unlike `ToneSequencer`.

### Changed
- The workspace builds its ESP-IDF examples against ESP-IDF v6.1 (was v5.3.3); the libraries still do not pin an ESP-IDF version.
- `.cargo/config.toml` (ESP-IDF pin, `ldproxy` linker, `build-std`, esp-hal linker scripts) is now committed, replacing `just setup-cargo-config`; Cargo never reads it for downstream users.
- CI: the host workflows call the shared `rustyfarian-ci` `host-checks.yml@v1`, and the device workflows use its shared setup actions.
- CI: one `just ci-<job>` recipe per job; `just act` runs the jobs locally.
- CI: a new `Cross-target IDF` workflow builds the ESP-IDF tier for ESP32, C3, C6 and S3, and the Xtensa toolchain moves to `1.97.0.0`.
- just: `pre-commit` is `fmt verify`, `verify` depends on `fmt-check`, and `ci` is `verify deny` (no longer `audit`, which runs as `just audit`) (family recipe contract).
- just: `setup-toolchain` → `setup esp` (family recipe contract).
- just: `setup-avr` → `setup avr` (family recipe contract).
- just: `act-list` → `act list`, and `act`'s `matrix` parameter folds into `job` (`just act idf:chip:c3`) (family recipe contract).
- just: `release-publish-tamer` → `release-publish`, and a new `semver` recipe runs `cargo semver-checks` (family recipe contract).
- bench: board key `feather-v2` → `esp32-feather`; the `feather-v2` board profile is unchanged (family bench catalogue).
- examples: the classic ESP32 chip token `esp` → `esp32` (`hal_esp32_b3f`, `idf_esp32_oled`, …), and `just ci-idf esp` → `just ci-idf esp32` (family chip tokens).

## [0.2.0] - 2026-11-12

### Added
- `signwriter` (new crate): a `no_std` driver for 128x64 I2C OLEDs that tells an SSD1306 from an SH1106 and survives hot-plug ([ADR-013](docs/adr/013-signwriter.md)).
- `tamer` sensors: an I2C `Mpu6050` driver with `TiltEstimator`, and an I2C `Bme680` driver that is experimental (not yet verified on hardware).
- `tamer` feature `hal-async`: `Bme680::measure_async` and `Mpu6050::calibrate_async` over the same sans-IO machines as the blocking runners ([ADR-010](docs/adr/010-shared-logic-ladder.md)).
- `tamer` primitives: `hotplug`, `burst`, `latch::OnceLatch`, `HallSensor::pole`, `ToneOutput::duty`, `AnalogCalibration::range_or`, `presence::SessionState`, and `mock::MockI2c`.
- `tamer::rotary`: the `RotaryEncoder` trait, `QuadratureDecoder::try_new`, and the `edge_count` / `skipped_count` diagnostics.
- `rustyfarian-esp-hal-peripherals`: the ISR-decoded `rotary::InterruptEncoder`, the async `rotary::Encoder`, and `analog::AdcReader`.
- `rustyfarian-esp-idf-peripherals`: `analog::AdcReader`, and `edge_count` / `skipped_count` on `rotary::Encoder`.
- Both tiers: board profiles (`board-*` features) that pick the I2C pins per board ([ADR-014](docs/adr/014-board-profiles.md)).
- Examples for button, rotary, OLED, MPU6050, BME680 and ADC across ESP32, C3, C6 and S3, and `tamer` on an Arduino Nano (`examples/avr-nano-tamer`).
- `just bench`: guided on-device checks through `rustyfarian-bench` ([ADR-011](docs/adr/011-adopt-rustyfarian-bench.md)), plus chip-matrix check and link recipes in CI.

### Changed
- BREAKING: `bme680::Error` and `mpu6050::Error` gain a `Sequence` variant.
- `tamer::rotary` positions wrap at the `i32` limits instead of panicking in debug builds.
- ESP32-C3 examples follow one pin map: I2C on SDA 6 / SCL 7, buzzer on GPIO 10 ([ADR-012](docs/adr/012-esp32-c3-pin-map.md)).

### Fixed
- ESP-IDF `rotary::Encoder`: no `critical_section` in the ISR, which crashed an ESP32-S3.
- ESP-IDF `rotary::Encoder`: no use-after-free on drop on dual-core chips, and no position drift from edges during `new`, `set_position` or `reset`.
- esp-hal ADC reads load the efuse init code, as ESP-IDF does.
- `tamer::bme680` decodes `par_h1`, `par_h2` and `range_sw_err` correctly.
- `tamer::mpu6050::AccelCalibration` sums no longer overflow on long runs.
- `tamer::presence` keeps `last_onset_age` exact across `restore`.

## [0.1.0] - 2026-09-26

First release: the pure `no_std` `tamer` core plus thin esp-hal and ESP-IDF hardware tiers.

### Added
- `tamer` digital inputs: `debounce`, `presence` (incl. `PresenceSession`), `rotary`, `button`, `touch` gestures.
- `tamer` analog and sensors: `analog`, `hall`, `smoothing`, `range_map`, `mpu6050`, and opt-in `tilt`.
- `tamer` outputs: `tone` sequencer and ITU-R `morse` keyer, both timing-agnostic.
- Optional `hal` feature with `embedded-hal` adapters and `MockInputPin` for host tests.
- `rustyfarian-esp-idf-peripherals`: interrupt-driven rotary `Encoder`, hardware-verified on ESP32-S3.
- `rustyfarian-esp-hal-peripherals`: skeleton that re-exports `tamer`, with chip features for C3/C6/ESP32/S3.
- ESP32-C3 examples on both tiers, plus `idf_s3_rotary`.

### Requirements
- `tamer` needs Rust 1.88+; the hardware tiers need 1.95+.
- Pinned to `esp-hal 1.2.2` and `esp-idf-hal 0.47.0` / `esp-idf-svc 0.53.0` / `esp-idf-sys 0.38.1`.
