# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning](https://semver.org/) (pre-1.0: minor
bumps may carry breaking changes).

---

## [Unreleased]

### Added
- `rustyfarian_esp_idf_peripherals::rotary::Encoder` — the esp-idf tier's first
  library driver (not a re-export). An interrupt-driven rotary encoder with a
  debounced push button, using persistent raw-FFI `gpio_isr_handler_add` (not
  HAL subscriptions, which are one-shot and unsuitable for quadrature). Per-instance
  heap-allocated ISR context (`Box<IsrContext>`), robust teardown with
  a critical-section barrier for dual-core safety. Delegates all decoding to
  `tamer::rotary::QuadratureDecoder` and `tamer::button::ButtonDecoder`.
  See [ADR-005](docs/adr/005-raw-ffi-persistent-interrupts.md) (raw FFI pattern)
  and [ADR-006](docs/adr/006-interrupt-encoder-instance-and-api-shape.md)
  (per-instance context and trait-readiness).
- `idf_s3_rotary` example — the crate's first ESP32-S3 example, exercising CW/CCW
  rotation and all five button events (Press, Release, Click, DoubleClick, LongPress)
  on real hardware (CrowPanel 1.28" / KY-040 encoder).
- `tamer::touch` — pure touch-panel event detection: `TouchTracker` turns
  per-frame `(Option<TouchPoint>, now)` samples into raw `Down`/`Move`/`Up`
  contact edges plus derived `Tap`/`LongPress`/`Swipe` gestures (at most one
  event per update; a lift emits `Up` and queues the terminal gesture for the
  next call). Chip-agnostic and clock-injected — works on controllers with no
  hardware gesture engine (e.g. the CYD's XPT2046); resistive `touched`
  flicker composes with `tamer::debounce::Debouncer` upstream. See
  [ADR-007](docs/adr/007-touch-event-detection.md) and
  [docs/features/touch-event-detection-v1.md](docs/features/touch-event-detection-v1.md).

### Changed
- `rustyfarian_esp_idf_peripherals` lib.rs documentation now distinguishes two
  interrupt patterns: polled one-shot HAL subscriptions (for low-frequency signals
  like button wakes) vs. persistent raw-FFI interrupts (for edge-dense inputs like
  encoders). Corrects the skeleton's implicit assumption that HAL subscriptions
  are universal. See `lib.rs` module docs and ADR-005 for details.

### Added (pre-driver documentation)
- `tamer` workspace skeleton: the pure `no_std` core plus thin
  `rustyfarian-esp-hal-peripherals` (esp-hal) and `rustyfarian-esp-idf-peripherals`
  (ESP-IDF) re-export tiers, an optional `embedded-hal` `hal` seam, tooling, CI,
  and dual MIT/Apache-2.0 licensing.
- `tamer::debounce` — `Debouncer`, `Edge`, `EdgeDetector`, and the `hal`-gated
  `DebouncedInput<P>` adapter (caller-owned `u64` clock; `try_from_pin`).
- `tamer::presence` — `Presence`, `Polarity`, `DigitalPresence`, and the
  `hal`-gated `DigitalPresenceInput<P>` adapter for polarity-aware debounced
  digital presence detection.
- `tamer::rotary` — `QuadratureDecoder`, `EncoderDirection`, and the `hal`-gated
  `QuadratureInput<A, B>` adapter (`try_from_pins`).
- `tamer::button` — `ButtonDecoder` and `ButtonEvent` (raw `Press`/`Release`
  edges plus layered `Click`/`DoubleClick`/`LongPress` gestures), and the
  `hal`-gated `ButtonInput<P>` adapter (active-low/high; `try_from_pin`).
- `tamer::analog` — `AnalogCalibration`, `AnalogRange`, `AnalogValue`,
  `AnalogInput<R>`, and `MockAnalogRead` for host-testable ADC calibration,
  normalization, and deadbanded analog movement.
- `tamer::mock::MockInputPin` (`hal`) — settable `InputPin` mock for host tests.
- ESP32-C3 B3F button examples on both esp tiers (`hal_c3_b3f`, `idf_c3_b3f`),
  wiring the first hardware dependency (`esp-hal` / `esp-idf-hal`) and the
  `build-example` / `run` / `check-hal` justfile recipes.
- ESP32-C3 potentiometer examples on both esp tiers (`hal_c3_poti`,
  `idf_c3_poti`) using ADC1 and `tamer::analog` startup calibration /
  normalization.
- `tamer::hall` — `HallSensor` for Hall-effect magnetic presence detection via ADC
  and linear sensor model, with `SlidingAverage` smoothing, startup calibration,
  and `set_midpoint` / `set_threshold` runtime control.
- ESP32-C3 Hall-effect examples: linear analog sensor via ADC (`hal_c3_hall_linear`,
  `idf_c3_hall_linear`; uses `tamer::hall` + calibration) and unipolar digital
  switch (`hal_c3_hall_switch`; KY-003 / A3144 module read via
  `tamer::presence::DigitalPresence`).
- `tamer::range_map` — `RangeMap`, a clamped linear remap from a `u16` analog
  reading to a `u8` output (e.g. ADC counts to LEDC PWM duty), with
  round-to-nearest scaling matching `AnalogRange::normalize` and an
  `inverted()` builder for controls where a rising input should produce a
  falling output.
- ESP32-C3 potentiometer-dimmed LED examples on both esp tiers (`hal_c3_poti_led`,
  `idf_c3_poti_led`) — the repo's first output/PWM examples. A potentiometer on
  ADC1 (GPIO 4) drives an external LED on GPIO 6 via 8-bit-resolution LEDC PWM,
  mapping raw ADC counts straight onto PWM duty with `tamer::range_map::RangeMap`.
- Primitives donated by clean reimplementation (relicensed to MIT OR Apache-2.0)
  from `rustyfarian-knob` and `rustbox-peripherals`; the button event contract
  intentionally diverges from the knob's, and digital presence follows the
  donor repo's accepted abstraction boundary — see
  [ADR-001](docs/adr/001-input-primitives-origin.md) and
  [ADR-002](docs/adr/002-digital-presence.md).
- `tamer::mpu6050` — MPU6050 IMU protocol constants, raw 14-byte burst parsing
  (`RawReading`, `parse_raw`), and accelerometer Y/Z offset calibration
  (`AccelCalibration`, `AccelOffsets`, `apply_offsets`); `tamer`'s first
  device-named module, donated by clean reimplementation (relicensed to MIT OR
  Apache-2.0) from `rustbox-peripherals`. `RawReading` is
  private-fields-plus-accessors (built only via `parse_raw`), `INIT_SEQUENCE` a
  slice, and the offset pipeline is `i32` throughout (overflow-safe). See
  [docs/features/mpu6050-imu-v1.md](docs/features/mpu6050-imu-v1.md).
- `tamer::smoothing::EmaFilter` — exponential moving average, the `f32` sibling
  of `SlidingAverage`; `new(alpha)` panics on out-of-range / `NaN` alpha.
- `tamer::tilt` (opt-in `tilt` feature, `dep:micromath`) — `tilt_degrees` /
  `tilt_degrees_i32`, scale-free two-axis inclination via `atan2`; `tamer`'s
  first floating-point surface, feature-gated so the default build stays
  dependency-free.
- ESP32-C3 I2C bus-scanner examples on both esp tiers (`hal_c3_i2c_scan`,
  `idf_c3_i2c_scan`) — the repo's first I2C examples: a bring-up diagnostic that
  probes `0x08..=0x77` (SDA GPIO 4 / SCL GPIO 5) and logs ACKing addresses, ahead
  of the upcoming MPU6050 hardware twin. The scan tallies non-NACK bus faults and
  warns if any occurred, so a shorted or pull-up-less bus is not misreported as an
  empty one. See [ADR-004](docs/adr/004-i2c-bus-pattern.md).
- `tamer::tone` — a pure tone/duration sequencer (melody player): `Note`,
  `SequenceMode`, `ToneOutput`, `SequenceEvent`, and `ToneSequencer<'notes>`,
  stepping a borrowed `&[Note]` table into re-readable `ToneOutput` values for a
  downstream buzzer/PWM/DAC adapter. `tamer`'s first output/actuator primitive;
  caller-owned `u64` tick contract mirroring `debounce`/`button`. See
  [docs/features/archive/tone-sequencer-v1.md](docs/features/archive/tone-sequencer-v1.md).
- ESP32-C3 piezo-buzzer examples on both esp tiers (`hal_c3_buzzer`,
  `idf_c3_buzzer`) — the first downstream consumer of `tamer::tone`: a
  `ToneSequencer` melody drives a passive piezo on GPIO 6 via LEDC PWM, retuning
  the timer frequency per note. All sequencing stays in the pure core; only the
  PWM writing lives in the example.
