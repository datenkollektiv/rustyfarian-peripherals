# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning](https://semver.org/) (pre-1.0: minor bumps may carry breaking changes).

---

## [Unreleased]

## [0.2.1] - 2026-11-22

### Added
- `tamer::ir` (opt-in `ir`): a data-driven IR pulse codec (`PulseCodec`, NEC preset and repeat helpers, `FrameAssembler`, `carrier_ticks`, `PulseSink` / `PulseSource` with mocks), bridged to the RMT by the esp-hal tier's `ir::IrTransmitter` / `IrReceiver` with examples `hal_c3_ir_tx` / `hal_c3_ir_rx` / `hal_c6_ir_tx` (NEC only, boot log with both carrier predictions) and bench check `c6/ir-tx`; compile-verified on all four chips; C6 → C3 NEC end to end verified on hardware, the RMT carrier frequency itself still unmeasured.
- `tamer::staged`: `StagedTimer` steps a borrowed `&[Stage]` table on a caller-driven clock in `StagedMode::Once` or `Loop`, `stage()` names the stage of the latest step, and an empty table reports `Finished` on the first update.
- `tamer::sht3x` (opt-in `sht3x`): CRC and fixed-point conversion, the sans-IO `SingleShot` machine, and the `Sht3x` driver (`hal`, `measure_async` with `hal-async`) that waits out a command left in flight; examples `{hal,idf}_{c3,c6}_sht3x`, verified on hardware on the C6 (`c6/sht3x`).
- `tamer::hx711` (opt-in `hx711`, experimental, not yet verified on hardware): sign extension, `Gain`, `Calibration` with a 16-byte codec, the `LoadCellScale<N>` weigh pipeline, `InterruptMask`, and the blocking `Hx711` driver (`hal`), which completes the pulse train clock-only on a pin fault.
- Both tiers' opt-in `hx711` feature ships `hx711::MaskedSection` (esp-hal `critical_section`, ESP-IDF `IsrCriticalSection`), with examples `{hal,idf}_{c3,c6}_hx711` and bench checks `c3/hx711` / `c6/hx711`.
- `smoothing::SignedSlidingAverage<N>`, `mock::MockOutputPin`, `just test-sht3x` / `test-hx711` (both in `verify` and `ci-test`), and `just build-all-examples [filter]` (family recipe contract J26).

### Changed
- ESP-IDF examples build against ESP-IDF v6.1 (was v5.3.3), and `.cargo/config.toml` is committed, replacing `just setup-cargo-config`; the libraries pin no ESP-IDF version (they still build against v5.3.3) and downstream Cargo never reads the file.
- CI runs on the shared `rustyfarian-ci` workflows and setup actions, one `just ci-<job>` per job (`just act` runs them locally), with a per-chip `Cross-target IDF` workflow and the Xtensa toolchain at `1.97.0.0`.
- Family conventions: `setup esp|avr`, `act list`, `release-publish`, a new `semver` recipe, `ci` = `verify deny`; chip token `esp` → `esp32`; bench board `feather-v2` → `esp32-feather`, runner `v0.6.0`, git-ignored `bench/bench.local.toml`.
- Bench pogo set ([ADR-015](docs/adr/015-pogo-ports.md)): rig pages list the pogos and button pin, the EC11 check moves to `c3-rotary/rotary`; README and AGENTS frame the repo as input and output peripherals; `deny` ignores RUSTSEC-2026-0543 until `esp-idf-sys` drops `build-time`.

### Fixed
- `IrReceiver::receive` no longer loses frames after a timeout on the ESP32-C3: it now waits the configured idle time, but at least 10 ms, after aborting a reception, so a call can take up to `timeout_ms` plus that wait.

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
