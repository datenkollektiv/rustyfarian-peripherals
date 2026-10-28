# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning](https://semver.org/) (pre-1.0: minor bumps may carry breaking changes).

---

## [Unreleased]

### Added
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

### Fixed
- ESP-IDF `rotary::Encoder::drop`: no use-after-free on dual-core chips (ESP32, ESP32-S3); see ADR-006.
- ESP-IDF `rotary::Encoder::set_position` / `reset`: a concurrent edge can no longer leave the position mirror and decoder out of step.
- ESP-IDF `rotary::Encoder::new`: a turn during construction no longer shifts the first detent.

### Changed
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
