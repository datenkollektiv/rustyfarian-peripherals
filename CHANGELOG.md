# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning](https://semver.org/) (pre-1.0: minor bumps may carry breaking changes).

---

## [Unreleased]

### Added

- `signwriter` (new workspace crate, released in lockstep with `tamer` as `0.2.0`): a `no_std`, allocation-free driver for 128x64 I2C OLEDs that detects an SSD1306 or an SH1106 by a RAM read-back (a rejected read after acknowledged writes means SSD1306; `Oled::with_controller` pins the controller and skips detection), initialises it, flushes only the dirty pages of an `embedded-graphics` `DrawTarget` and reports attach and detach through `tamer::hotplug` (`Oled`, `Controller`, `Event`, `Error`); see [ADR-013](docs/adr/013-signwriter.md) and [Feature: signwriter v1](docs/features/signwriter-v1.md).
- Examples `hal_c3_oled` and `idf_c3_oled` and bench scenario `c3/oled`: a 128x64 OLED (SSD1306 or SH1106) on the C3 I2C pins (SDA 6 / SCL 7, 0x3C) that can be attached after boot and swapped while running (`tamer::hotplug`), drawn with `embedded-graphics` through `signwriter` (dev-dependencies only, per ADR-008).
- Example `hal_c3_rig_check` and bench scenario `c3/rig`: verifies the C3 bench rig against the ADR-012 pin map one connector at a time (I2C device attached and detached on 6/7, a driven part on the GPIO 4 pogo found by a pull test, the button on GPIO 3, the buzzer on GPIO 10 confirmed by a button press when the tone sounds); only GPIO 10 is driven.
- `rustyfarian-esp-hal-peripherals::analog::AdcReader` and `AdcError`: one-line ADC1 one-shot reader for esp-hal returning raw counts (`AdcReader::new(adc1, pin)`, infallible), fixed at 11 dB, loading the efuse init code via `AdcCalBasic` on C3/C6/S3 (classic ESP32 has none and reads uncalibrated, as ESP-IDF does); implements `tamer::analog::AnalogRead`.
- `rustyfarian-esp-idf-peripherals::analog::AdcReader` and `AdcError`: one-line ADC1 one-shot reader for ESP-IDF returning raw counts (`AdcReader::new(adc1, pin)?`), fixed at `DB_12`; implements `tamer::analog::AnalogRead`, and `AdcError` wraps `EspError` with `source()`.
- ADR-010 adopted: shared logic follows the three-rung ladder (a generic `embedded-hal` driver first, an event machine next, and a sans-IO action machine only on a trigger), and the `Bme680` and `Mpu6050` runners now sit over the `ForcedConversion` and `OffsetSampler` machines; the hardware re-runs that the adoption needs are gated in `release-plan.md` § Final hardware pass.
- Examples `hal_c3_bme680`, `idf_c3_bme680` and embassy `hal_c3_bme680_async` (esp-hal `I2c` in `Async` mode, `Bme680::measure_async`): temperature, pressure, humidity and gas on the C3 I2C pins (GPIO6/GPIO7, 100 kHz, ADR-012); `docs/hardware-setup.md` gains the shared I2C pogo header wiring (BME680, I2C OLED, MPU6050 on one 4-pin header).
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
- `hal_c3_mpu6050_tilt` / `idf_c3_mpu6050_tilt`: ESP32-C3 examples on I2C0 (GPIO 6 SDA / GPIO 7 SCL, 100 kHz) that check `WHO_AM_I`, calibrate at rest, and print the tilt angle; the esp-hal twin is hardware-verified on an ESP32-C3 SuperMini, the ESP-IDF twin compile-verified.
- `just check-matrix`, with `check-matrix-hal` and `check-matrix-idf` and the per-chip forms `check-matrix-hal-chip <chip>` / `check-matrix-idf-chip <chip>`: check and clippy both tier libraries on ESP32, C3, C6 and S3, one chip at a time; not part of `just verify`.
- `just setup-avr`, `just check-avr-target`, `just build-avr-example` and `just build-avr-example-upstream`: install the pinned AVR nightly, check `tamer --features hal` on `avr-none`, build the AVR example, and run a throwaway build against avr-hal `main`.
- `examples/avr-nano-tamer`: excluded from the workspace, an Arduino Nano demo that debounces a button on D2 through `tamer::ButtonInput`, toggles D13 on each click and prints events at 57600 baud.
- `just build-matrix` (with `build-matrix-hal` / `build-matrix-idf`): builds and links `{hal,idf}_{esp,c3,c6,s3}_b3f`, because `check-matrix` never links; the RISC-V and Xtensa CI jobs link their chips' `hal_*_b3f` the same way.
- `scripts/ramdisk-guard.sh`: shared check that stops the matrix loops below 4 GiB free on the RAM disk; it never prunes.
- `scripts/check-matrix.sh` and `scripts/build-avr-example-upstream.sh`; `scripts/lib.sh` accepts the `esp32c3` and `esp32c6` chip tokens.
- CI workflows `cross-target-riscv.yml`, `cross-target-xtensa.yml`, `cross-target-avr.yml` and `cross-target-avr-upstream.yml`: path-filtered PR gates for the esp-hal chips and the AVR link, plus a weekly upstream AVR canary; each job runs `just` recipes.
- `hal_esp_rotary_isr`, `hal_c3_rotary_isr` and `hal_c6_rotary_isr`: ISR-decoded rotary examples for ESP32, C3 and C6 (esp-hal).
- `idf_esp_rotary`, `idf_c3_rotary` and `idf_c6_rotary`: ISR-decoded rotary examples for ESP32, C3 and C6 (ESP-IDF), printing the same event lines as the esp-hal twins.
- `idf_esp_b3f`, `idf_c6_b3f` and `idf_s3_b3f`: button smoke examples on ESP32, C6 and S3 (ESP-IDF), matching the esp-hal `b3f` examples.
- `just bench <board> [check] [flags]`, `just bench <example>`, `just bench-list` and `just bench-detect`: guided, rerunnable hardware checks that build, flash, reset and judge each step with an automatic verdict; see `bench/README.md` and ADR-011.
- `bench/` layout (ADR-011): one directory per board, one TOML scenario per check (`<board>/<check>.toml`), `bench/bench.toml` for build, flash and board settings, and git-ignored `results.jsonl` and logs; the `s3` board is in progress, and the `c3`, `c6` and `esp32` scenarios are written but not run.
- Bench scenarios: `c3` (`b3f`, `rotary`, `bme680`, `bme680-async`, `mpu6050`, `buzzer`, `hall-linear`, `poti`), `c6` and `esp32` (`b3f`, `rotary`); `s3` keeps `b3f`, `rotary` and `rotary-isr`, and on `c3`, `c6` and `esp32` the `rotary` scenario runs the hal ISR example and its idf twin.
- Compile-time tier conformance guards in both tier crates: the esp-hal and ESP-IDF rotary `Encoder` must keep `edge_count()` and `skipped_count()` in parity, and both crates must keep re-exporting the shared `tamer` types.
- Board profiles on both hardware tiers ([ADR-014](docs/adr/014-board-profiles.md)): one `board-*` Cargo feature per board (`board-c3-supermini`, `board-c6-devkitc`, `board-esp32-wroom`, `board-feather-v2`) and a public `board` module with `board_i2c!`, `board_i2c_power!`, `BOARD_NAME` and `I2C_PINS`; `just build-example <ex> <profile>` (also `just flash` and `just run`) overrides the default profile of each chip, and two profiles or a chip mismatch fail to compile.
- Examples `hal_c6_oled`, `idf_c6_oled`, `hal_esp_oled` and `idf_esp_oled`: a 128x64 OLED on the I2C pins of the board profile (C6-DevKitC-1 SDA 21 / SCL 22; ESP32 SDA 21 / SCL 22 by default, or the Feather V2 with `feather-v2 as the second argument`, SDA 22 / SCL 20 with socket power on GPIO 2, not yet run on hardware), drawn through `signwriter`.

### Fixed

- `hal_c3_hall_linear`, `hal_c3_poti` and `hal_c3_poti_led` read the ADC through `AdcCalBasic`, which loads the efuse zero-volt init code as ESP-IDF does; uncalibrated, esp-hal 1.2.2 writes init code 0 and a C3 read a 1.67 V Hall rest point as ~4030, clipping one pole (the idf twin read 2345 on the same wiring).
  The fix is now enforced by `AdcReader`, which always loads the init code where the chip has one, so no ADC example can regress to the uncalibrated read.
- `tamer::mpu6050::AccelCalibration`: sums are `i64`, so long calibration runs (over ~65,500 samples at full scale) no longer overflow (debug panic, release wrap) and the average divides by the full `u32` count instead of a cast that went negative above `i32::MAX`; samples beyond `u32::MAX` are ignored.
- ESP-IDF `rotary::Encoder`: the GPIO ISR no longer takes `critical_section`, which on ESP-IDF is a FreeRTOS mutex and crashed an ESP32-S3 (`xTaskRemoveFromEventList` in ISR context) under cross-core construct/drop; it now uses esp-idf-hal's ISR-safe `interrupt::IsrCriticalSection`. See ADR-006.
- ESP-IDF `rotary::Encoder::drop`: no use-after-free on dual-core chips (ESP32, ESP32-S3); see ADR-006.
- ESP-IDF `rotary::Encoder::set_position` / `reset`: a concurrent edge can no longer leave the position mirror and decoder out of step.
- ESP-IDF `rotary::Encoder::new`: a turn during construction no longer shifts the first detent.

### Changed
- The ADC examples `hal_c3_hall_linear`, `hal_c3_poti`, `hal_c3_poti_led`, `idf_c3_hall_linear`, `idf_c3_poti` and `idf_c3_poti_led` set up the ADC in one line with `AdcReader` and read with `read_raw()`; the hal twins now log the `OnceLatch` read errors like the idf twins, and bench log text is unchanged.
- BREAKING: `bme680::Error` and `mpu6050::Error` gain a `Sequence` variant (the driver and its state machine fell out of step; a library bug). `Bme680::measure` and `Mpu6050::calibrate` return `Err(Error::Sequence)` when the machine answers `OutOfPhase` or the poll fuel runs out, replacing the `Timeout` and zero-offset `Ok` fallbacks; the `debug_assert!` is removed. `is_nack()` is false for it. Exhaustive matches on either `Error` must add an arm.
- `Mpu6050::calibrate` now runs as a bounded blocking runner over `OffsetSampler`; signature, errors, I2C transactions and delay calls are unchanged.
- `tamer::bme680::Bme680::measure` now runs as a thin blocking runner over `ForcedConversion`; signature, errors, I2C transaction order and `DelayNs` calls are unchanged.
- `idf_c3_passive_buzzer_arpeggio` now uses 10-bit LEDC resolution (was 8-bit) to match the hal twins and shares `ToneOutput::duty`; the documented ~77 Hz lower bound is computed, not yet verified on a device.
- `hal_c3_tilt_motion` / `idf_c3_tilt_motion` examples use `tamer::burst::BurstDetector` instead of a hand-written burst detector; the per-transition `count=` field is dropped from the log line.
- `tamer::rotary`: position wraps at the `i32` limits instead of panicking in debug builds.
- `tamer::presence`: `PresenceSession::restore` / `snapshot` keep `last_onset_age` exact when the restored age exceeds the restore tick, instead of collapsing it to `now`.
  `last_onset()` still saturates at `0` in that case.
- Docs: the ESP-IDF `rotary::Encoder` IRAM caveat (README, rustdoc, ROADMAP) now states the verified behaviour — edges during a flash write are deferred and coalesced (a dropped detent), and the device crashes only if the GPIO ISR service is installed with `ESP_INTR_FLAG_IRAM` elsewhere. No code change.
- `scripts/flash.sh` port handling: an optional fourth argument names the serial port, which is passed to `espflash` as `--port` with `--non-interactive`; with no port, `espflash` picks or asks for the board as before.
- The rotary examples on both tiers (`idf_{esp,c3,c6,s3}_rotary`, `hal_{esp,c3,c6}_rotary_isr`, `hal_s3_rotary`) print one line format, `rotated CW position 3 (edges 12, skipped 0)`, read from `edge_count()` and `skipped_count()`, so one bench pattern covers both tiers.
- The `just bench` recipes run the `rustyfarian-bench` runner pinned to `v0.5.0` (`uvx --from git+https://github.com/datenkollektiv/rustyfarian-bench@v0.5.0`); `RUSTYFARIAN_BENCH=/path/to/rustyfarian-bench` selects a local checkout instead.
- Bench scenarios mark the steps that need a person with `attended = true`, so `just bench <board> --unattended` and `--attended` split a board pass; every `b3f` and `rotary` check starts with a hands-off 5 s no-panic watch, and the `s3/rotary` and `s3/rotary-isr` banner steps read the reset backlog (`arm = false`).

- ESP32-C3 examples follow one pin map (ADR-012), matching the C3 bench rig: I2C SDA 6 / SCL 7 (was 4 / 5, ADR-004 amended), buzzer GPIO 10 (was 6), B3F button GPIO 3 (was 4), rotary click GPIO 5 (was 10), external LED on GPIO 8 active-low (`poti_led` was 6, `blink` was 7); analog and 3-pin sensors stay on GPIO 4. Rewire C3 setups accordingly; no library API changes.
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
