# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning](https://semver.org/) (pre-1.0: minor bumps may carry breaking changes).

---

## [Unreleased]

### Added

- Examples `hal_c3_bme680`, `idf_c3_bme680` and embassy `hal_c3_bme680_async` (esp-hal `I2c` in `Async` mode, `Bme680::measure_async`): temperature, pressure, humidity and gas on the ADR-004 bus (GPIO4/GPIO5, 100 kHz); `docs/hardware-setup.md` gains the shared I2C pogo header wiring (BME680, I2C OLED, MPU6050 on one 4-pin header).
- `tamer` feature `hal-async` (implies `hal`, not default; adds the `embedded-hal-async` dependency): `Bme680::measure_async` and `Mpu6050::calibrate_async`, async runners over the same `ForcedConversion` / `OffsetSampler` machines with identical bus traffic, error mapping (including `Error::Sequence`) and poll-fuel bound; dropping the future mid-measure leaves the sensor converting and the next call's sleep-mode recovery handles it; `embedded_hal_async::i2c::I2c` for `MockI2c` and async `DelayNs` for `NoopDelay` under the same feature; parity tests need no executor.
- `tamer::mpu6050::OffsetSampler` (`new`, `poll`, `is_finished`, `max_polls`), `SamplerInput` and `SamplerAction`: sans-io rung-3 action machine for accelerometer offset calibration (ADR-010); available without `hal`; the enums are exhaustive, so adding a variant is breaking; `AccelCalibration` now also derives `PartialEq, Eq`.
- `tamer::bme680::ForcedConversion` (`new`, `poll`, `is_finished`, `required_ms`, `ambient_c`, `max_polls`), `ConversionInput`, `ConversionAction`, `ConversionError` and `required_ms(..)`: sans-io rung-3 action machine for one forced-mode conversion (ADR-010); compiles with `bme680` alone, no `hal`; the enums are exhaustive, so adding a variant is breaking; `impl From<ConversionError> for bme680::Error<E>` under `hal`.
- `tamer::analog::AnalogCalibration::range_or(min_span, fallback)`: observed range if it spans at least `min_span` counts, otherwise the fallback; the poti and poti-LED examples (hal and idf) use it.
- `tamer::latch::OnceLatch` (`trip`, `rearm`): report-once latch that reports the first event of a run and the end of that run; the poti, poti-LED and linear Hall examples (hal and idf) use it to print ADC read failures and recovery once.
- `tamer::hall::HallSensor::pole(raw)` and `Pole::{North, South, Absent}`: pole classification (absent field is `Absent`, strictly above the midpoint is `South`, otherwise `North`); the hal and idf linear Hall examples now only print it.
- `tamer::tone::ToneOutput::duty(max_duty)`: pure PWM duty for a square-wave buzzer (0 for a rest, otherwise half of `max_duty` scaled by amplitude/255, round-to-nearest, overflow-free); the hal and idf passive-buzzer arpeggio examples now call it and use the same 10-bit LEDC resolution.
- `tamer::rotary::QuadratureDecoder::try_new` and `DetentStepsError`: fallible constructor that rejects `steps_per_detent == 0`; the esp-hal and esp-idf encoder tiers now share it and only map the error to `EncoderError::InvalidConfig`.
- `tamer::rotary::QuadratureDecoder::edge_count()`: wrapping count of fed samples that differ from the previous one; `update` feeds repeats harmlessly, so the esp-hal `Encoder` / `InterruptEncoder` no longer keep their own last-levels and counter bookkeeping.
- `rustyfarian-esp-idf-peripherals::rotary::Encoder::edge_count()`: diagnostic pass-through of `QuadratureDecoder::edge_count`, read under the ISR-safe section like `skipped_count` (ADR-010 tier parity).
- `rustyfarian-esp-idf-peripherals::rotary::Encoder::skipped_count()`: diagnostic pass-through of `QuadratureDecoder::skipped_count`, matching the esp-hal `Encoder` and `InterruptEncoder` (ADR-010 tier parity).
- `tamer::burst::BurstDetector` / `BurstEvent::{Burst, Quiet}`: pure transition-burst event machine (`update(transition, now_ms) -> Option<BurstEvent>`) counting transitions in a window with a quiet timeout, usable over debounced presence transitions; re-exported at the `tamer` root and `tamer::prelude`.
- `tamer::rotary::RotaryEncoder` trait (`update`, `position`, `set_position`, `reset`, `is_button_pressed`) and `NoopRotaryEncoder` mock, re-exported at the `tamer` root and `tamer::prelude`; implemented by `rustyfarian-esp-idf-peripherals::rotary::Encoder` and `rustyfarian-esp-hal-peripherals::rotary::InterruptEncoder` for tier-agnostic app code.
  The async esp-hal `rotary::Encoder` intentionally does not implement it.
- `rustyfarian-esp-idf-peripherals::rotary` now re-exports `RotaryEncoder` and `NoopRotaryEncoder`.
- `rustyfarian-esp-hal-peripherals::rotary::InterruptEncoder`: sync, ISR-decoded quadrature encoder with the ESP-IDF `Encoder` method surface, so `position()` keeps up under blocking work; CrowPanel S3 example `hal_s3_rotary_isr`.
- `rustyfarian-esp-hal-peripherals::rotary::on_gpio_interrupt()`: GPIO interrupt hook that the app calls from its single esp-hal GPIO handler; a no-op when no `InterruptEncoder` is alive.
- `rustyfarian-esp-hal-peripherals::rotary::EncoderError::AlreadyInUse`: returned when a second `InterruptEncoder` is constructed while one is alive.
- `just capture <seconds> <logfile> [port]`: non-interactive serial capture for timed hardware runs.
- `just clippy-hal` / `just doc-hal`: lint (C3 + S3, lib and examples) and rustdoc (C3) gates for the esp-hal crate, denying warnings.
- `tamer::hotplug`: `Hotplug` / `Transition` pure attach/detach state machine for a removable device (e.g. an OLED on a pogo connector), fed per-probe `ok: bool`; attach after a continuous debounce window, detach after N consecutive failures, `reset()` and `is_attached()`.
- `rustyfarian-esp-hal-peripherals::rotary::Encoder`: async quadrature decoder for esp-hal with `wait_for_step()` edge futures (embassy-agnostic core, `embassy` feature for example runtime), fed by `tamer`'s decoders; CrowPanel S3 example `hal_s3_rotary`, hardware-verified on ESP32-S3.
- `tamer::presence`: `SessionState` with `PresenceSession::snapshot` / `restore`, `SessionState::aged_by`, and `PresenceSession::warmup_end` for resuming a session across a deep-sleep tick reset, and `PresenceSession::is_idle` (Armed and `Absent`) as the sleep-entry check.
- `hal_c6_passive_buzzer_arpeggio`: ESP32-C6 variant of the esp-hal passive-piezo arpeggio example.
- `just check-hal`: added ESP32-S3 line with `cargo +esp -Zbuild-std=core,alloc` check for Xtensa bare-metal drivers.
- `just ramdisk prune [days]`: removes idle per-repo target trees from the shared RAM disk.
- `tamer::rotary`: `QuadratureDecoder::skipped_count()` (and `QuadratureInput::skipped_count()`) — monotonic, wrapping count of 2-bit Gray-code jumps, so a starved consumer can detect missed samples; `update` results are unchanged.
- `rustyfarian-esp-hal-peripherals::rotary::Encoder::skipped_count()` pass-through; the module docs now state the measured blocking-work budget (~10 ms per step) between `wait_for_step` calls.
- `tamer::mpu6050::Mpu6050<I2C>` I2C driver behind `hal`: `init`, `who_am_i`, `read_raw`, `calibrate`, and `release`; errors are `mpu6050::Error` (`Bus`, `WrongIdentity`) with `Error::is_nack()`.
- `TiltEstimator` (feature `tilt`), re-exported at the `tamer` root and `tamer::prelude`: EMA-smoothed tilt angle from `RawReading` against calibrated offsets, with `update` and `reset`.
- `tamer::mock::MockI2c`, `MockI2cError`, `Transaction`, and `NoopDelay` under `hal`, for scripted host-side bus and delay tests.
- `tamer::bme680` behind the new `bme680` feature (no dependency): BME680 register map, `Oversampling` (`X1`..`X16`, no skip: a skipped channel yields a marker value and breaks `t_fine`) / `IirFilter` enums, control-byte builders (`ctrl_gas_1_byte(run_gas)` always uses heater set-point 0), status-bit helpers, `CalibrationData::from_raw` (rejects all-`0x00` / all-`0xFF` blocks with `CalibrationError`), `parse_raw`, `compensate` into `Reading` (°C, hPa, %RH, `Option` Ω), `res_heat`, `gas_wait`, and `measurement_duration_us`, transcribed from the Bosch BME68x SensorAPI v4.4.8 float path and checked against 18 reference vectors produced by running Bosch's own v4.4.8 C code.
- `tamer::bme680::Bme680<I2C>` blocking driver (features `bme680` + `hal`, also re-exported as `tamer::Bme680` and in the prelude) with `Error<E>` (`Bus`, `WrongChipId`, `UnsupportedVariant`, `Calibration`, `Timeout`, `BudgetTooShort`, `is_nack()`): `new` soft-resets, checks the chip ID, reads calibration and rejects a non-BME680 variant (the variant register is the last byte of the calibration block), handing the bus back on failure; `measure` first forces sleep mode (recovering from a previous `Timeout`), then runs one forced-mode conversion with a `DelayNs`-only `timeout_ms` budget and a pre-trigger data-block drain, and `release` returns the bus. The drain's `new_data` behaviour is pending hardware confirmation.
  `Oversampling` and `IirFilter` are re-exported at the `tamer` root and `tamer::prelude`.
- `just test-bme680`: host tests for the `bme680` feature alone and together with `hal` (the `bme680`-without-`hal` combination is not covered by `--all-features`).
- `tamer::bme680` fixes two calibration misdecodes from the donated source: `par_h1` / `par_h2` now take the correct nibbles of `E2`, and `range_sw_err` is sign-extended; `par_gh1..3` are parsed for `res_heat`.
- The `hal` feature now also covers `embedded_hal::i2c::I2c` and `DelayNs`, not only the `InputPin` adapters.
- `hal_c3_mpu6050_tilt` / `idf_c3_mpu6050_tilt`: ESP32-C3 examples on I2C0 (GPIO 4 SDA / GPIO 5 SCL, 100 kHz) that check `WHO_AM_I`, calibrate at rest, and print the tilt angle; the esp-hal twin is hardware-verified on an ESP32-C3 SuperMini, the ESP-IDF twin compile-verified.
- `just check-matrix`, with `check-matrix-hal` and `check-matrix-idf` and the per-chip forms `check-matrix-hal-chip <chip>` / `check-matrix-idf-chip <chip>`: check and clippy both tier libraries on ESP32, C3, C6 and S3, one chip at a time; not part of `just verify`.
- `just setup-avr`, `just check-avr-target`, `just build-avr-example` and `just build-avr-example-upstream`: install the pinned AVR nightly, check `tamer --features hal` on `avr-none`, build the AVR example, and run a throwaway build against avr-hal `main`.
- `examples/avr-nano-tamer`: excluded from the workspace, an Arduino Nano demo that debounces a button on D2 through `tamer::ButtonInput`, toggles D13 on each click and prints events at 57600 baud.
- `scripts/check-matrix.sh` and `scripts/build-avr-example-upstream.sh`; `scripts/lib.sh` accepts the `esp32c3` and `esp32c6` chip tokens.
- CI workflows `cross-target-riscv.yml`, `cross-target-xtensa.yml`, `cross-target-avr.yml` and `cross-target-avr-upstream.yml`: path-filtered PR gates for the esp-hal chips and the AVR link, plus a weekly upstream AVR canary; each job runs `just` recipes.
- `hal_esp_rotary_isr`, `hal_c3_rotary_isr` and `hal_c6_rotary_isr`: ISR-decoded rotary examples for ESP32, C3 and C6 (esp-hal).
- `idf_esp_rotary`, `idf_c3_rotary` and `idf_c6_rotary`: ISR-decoded rotary examples for ESP32, C3 and C6 (ESP-IDF), printing the same event lines as the esp-hal twins.
- `idf_esp_b3f`, `idf_c6_b3f` and `idf_s3_b3f`: button smoke examples on ESP32, C6 and S3 (ESP-IDF), matching the esp-hal `b3f` examples.

### Fixed

- `tamer::mpu6050::AccelCalibration`: sums are `i64`, so long calibration runs (over ~65,500 samples at full scale) no longer overflow (debug panic, release wrap) and the average divides by the full `u32` count instead of a cast that went negative above `i32::MAX`; samples beyond `u32::MAX` are ignored.
- ESP-IDF `rotary::Encoder`: the GPIO ISR no longer takes `critical_section`, which on ESP-IDF is a FreeRTOS mutex and crashed an ESP32-S3 (`xTaskRemoveFromEventList` in ISR context) under cross-core construct/drop; it now uses esp-idf-hal's ISR-safe `interrupt::IsrCriticalSection`. See ADR-006.
- ESP-IDF `rotary::Encoder::drop`: no use-after-free on dual-core chips (ESP32, ESP32-S3); see ADR-006.
- ESP-IDF `rotary::Encoder::set_position` / `reset`: a concurrent edge can no longer leave the position mirror and decoder out of step.
- ESP-IDF `rotary::Encoder::new`: a turn during construction no longer shifts the first detent.

### Changed
- BREAKING: `bme680::Error` and `mpu6050::Error` gain a `Sequence` variant (the driver and its state machine fell out of step; a library bug). `Bme680::measure` and `Mpu6050::calibrate` return `Err(Error::Sequence)` when the machine answers `OutOfPhase` or the poll fuel runs out, replacing the `Timeout` and zero-offset `Ok` fallbacks; the `debug_assert!` is removed. `is_nack()` is false for it. Exhaustive matches on either `Error` must add an arm.
- `Mpu6050::calibrate` now runs as a bounded blocking runner over `OffsetSampler`; signature, errors, I2C transactions and delay calls are unchanged.
- `tamer::bme680::Bme680::measure` now runs as a thin blocking runner over `ForcedConversion`; signature, errors, I2C transaction order and `DelayNs` calls are unchanged.
- `idf_c3_passive_buzzer_arpeggio` now uses 10-bit LEDC resolution (was 8-bit) to match the hal twins and shares `ToneOutput::duty`; the documented ~77 Hz lower bound is computed, not yet verified on a device.
- `hal_c3_tilt_motion` / `idf_c3_tilt_motion` examples use `tamer::burst::BurstDetector` instead of a hand-written burst detector; the per-transition `count=` field is dropped from the log line.
- `tamer::rotary`: position wraps at the `i32` limits instead of panicking in debug builds.
- `tamer::presence`: `PresenceSession::restore` / `snapshot` keep `last_onset_age` exact when the restored age exceeds the restore tick, instead of collapsing it to `now`.
  `last_onset()` still saturates at `0` in that case.
- Docs: the ESP-IDF `rotary::Encoder` IRAM caveat (README, rustdoc, ROADMAP) now states the verified behaviour — edges during a flash write are deferred and coalesced (a dropped detent), and the device crashes only if the GPIO ISR service is installed with `ESP_INTR_FLAG_IRAM` elsewhere. No code change.

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
