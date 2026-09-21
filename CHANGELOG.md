# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning](https://semver.org/) (pre-1.0: minor
bumps may carry breaking changes).

---

## [Unreleased]

Pre-1.0 groundwork: the workspace skeleton, the pure `tamer` core, the two esp
tiers, and the first hardware driver. Entries are grouped by theme; per-module
detail lives in the module docs, the ADRs, and the feature docs linked below.

### Added
- `tamer` workspace skeleton — the pure `no_std` core plus thin
  `rustyfarian-esp-hal-peripherals` (esp-hal) and `rustyfarian-esp-idf-peripherals`
  (ESP-IDF) tiers, an optional `embedded-hal` `hal` seam with `MockInputPin`,
  tooling, CI, and dual MIT/Apache-2.0 licensing.
- Digital input primitives — `tamer::debounce`, `tamer::presence`,
  `tamer::rotary`, and `tamer::button`, each with its `hal`-gated
  `try_from_pin(s)` adapter, donated by clean reimplementation from
  `rustyfarian-knob` and `rustbox-peripherals` with an intentionally divergent
  button event contract (see [ADR-001](docs/adr/001-input-primitives-origin.md),
  [ADR-002](docs/adr/002-digital-presence.md)).
- Analog and sensor primitives — `tamer::analog` (calibration, normalization,
  deadbanding), `tamer::hall` (linear Hall model), `tamer::smoothing`
  (`SlidingAverage`, `EmaFilter`), `tamer::range_map` (clamped `u16` → `u8`
  remap), `tamer::mpu6050` (burst parsing plus accel offset calibration), and
  the opt-in `tilt` feature (`dep:micromath`, `tamer`'s only floating-point
  surface) — see
  [docs/features/mpu6050-imu-v1.md](docs/features/mpu6050-imu-v1.md).
- `tamer::touch` — pure touch-panel event detection turning per-frame
  `(Option<TouchPoint>, now)` samples into `Down`/`Move`/`Up` edges plus derived
  `Tap`/`LongPress`/`Swipe` gestures, chip-agnostic and clock-injected for
  controllers with no hardware gesture engine (see
  [ADR-007](docs/adr/007-touch-event-detection.md),
  [feature doc](docs/features/touch-event-detection-v1.md)).
- `tamer::presence::PresenceSession` — warm-up-gated session state machine over
  a debounced `Presence` level that hides latched-sensor boot pulses, arms on
  the first observed `Absent`, then emits `Started`/`Ended` with `last`/`max`
  dwell tracking (see
  [feature doc](docs/features/archive/presence-session-v1.md)).
  Arming *always* requires an observed `Absent` sample: even with
  `warmup == 0`, a sensor already `Present` on the first poll is suppressed as
  `Settling` and starts no session.
- Output primitives — `tamer::tone` (a `ToneSequencer` stepping a borrowed
  `&[Note]` table into re-readable `ToneOutput` values) and `tamer::morse` (an
  ITU-R M.1677-1 on/off keyer), both timing-agnostic and host-testable with no
  GPIO/PWM coupling (see
  [tone](docs/features/archive/tone-sequencer-v1.md),
  [morse](docs/features/archive/morse-keyer-v1.md)).
- `rustyfarian_esp_idf_peripherals::rotary::Encoder` — the esp-idf tier's first
  library driver: an interrupt-driven rotary encoder with debounced push button
  using persistent raw-FFI `gpio_isr_handler_add`, per-instance heap-allocated
  ISR context, and critical-section teardown for dual-core safety, delegating
  all decoding to `tamer` (see
  [ADR-005](docs/adr/005-raw-ffi-persistent-interrupts.md),
  [ADR-006](docs/adr/006-interrupt-encoder-instance-and-api-shape.md)).
- ESP32-C3 example twins on both esp tiers — `b3f` (button), `poti` and
  `poti_led` (ADC plus LEDC PWM), `hall_linear` and `hall_switch`, `i2c_scan`
  (bus-scanner bring-up diagnostic, see [ADR-004](docs/adr/004-i2c-bus-pattern.md)),
  `passive_buzzer_arpeggio`, `active_buzzer_morse`, and `blink`, plus
  `idf_s3_rotary` (the first ESP32-S3 example, hardware-verified on a CrowPanel
  1.28" / KY-040 encoder) and the `build-example` / `run` / `check-hal`
  justfile recipes.

### Changed
- `rustyfarian_esp_idf_peripherals` lib.rs documentation now distinguishes two
  interrupt patterns: polled one-shot HAL subscriptions (for low-frequency signals
  like button wakes) vs. persistent raw-FFI interrupts (for edge-dense inputs like
  encoders). Corrects the skeleton's implicit assumption that HAL subscriptions
  are universal. See `lib.rs` module docs and ADR-005 for details.
- Passive-piezo arpeggio examples renamed to disambiguate by buzzer type and
  pattern: `hal_c3_buzzer` → `hal_c3_passive_buzzer_arpeggio` and
  `idf_c3_buzzer` → `idf_c3_passive_buzzer_arpeggio`. New active-buzzer Morse
  examples use GPIO on/off keying (not PWM), making the distinction explicit.
