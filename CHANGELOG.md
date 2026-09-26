# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project follows [Semantic Versioning](https://semver.org/) (pre-1.0: minor bumps may carry breaking changes).

---

## [Unreleased]

Pre-1.0 groundwork: the workspace skeleton, the pure `tamer` core, the two esp tiers, and the first hardware driver.
Per-module detail lives in the module docs, the ADRs, and the linked feature docs.

### Added
- `tamer` workspace skeleton: the pure `no_std` core, thin `rustyfarian-esp-hal-peripherals` / `rustyfarian-esp-idf-peripherals` tiers, an optional `embedded-hal` `hal` seam with `MockInputPin`, tooling, CI, and dual MIT/Apache-2.0 licensing.
- Digital input primitives `tamer::{debounce, presence, rotary, button}`, each with a `hal`-gated `try_from_pin(s)` adapter, reimplemented from `rustyfarian-knob` / `rustbox-peripherals` with a deliberately divergent button contract ([ADR-001](docs/adr/001-input-primitives-origin.md), [ADR-002](docs/adr/002-digital-presence.md)).
- Analog and sensor primitives `tamer::{analog, hall, smoothing, range_map, mpu6050}` plus the opt-in `tilt` feature, `tamer`'s only floating-point surface ([feature doc](docs/features/mpu6050-imu-v1.md)).
- `tamer::touch` gesture detection (`Down`/`Move`/`Up` edges, `Tap`/`LongPress`/`Swipe`) and `tamer::presence::PresenceSession` warm-up-gated session tracking with dwell ([ADR-007](docs/adr/007-touch-event-detection.md), [touch](docs/features/archive/touch-event-detection-v1.md), [session](docs/features/archive/presence-session-v1.md)).
- `PresenceSession` arming always requires an observed `Absent` sample, so even with `warmup == 0` a sensor already `Present` on the first poll is suppressed as `Settling` and starts no session.
- Output primitives `tamer::tone` (`ToneSequencer` over a borrowed `&[Note]` table) and `tamer::morse` (ITU-R M.1677-1 on/off keyer), timing-agnostic with no GPIO/PWM coupling ([tone](docs/features/archive/tone-sequencer-v1.md), [morse](docs/features/archive/morse-keyer-v1.md)).
- `rustyfarian_esp_idf_peripherals::rotary::Encoder`, an interrupt-driven encoder with debounced button on persistent raw-FFI ISRs and critical-section teardown, hardware-verified on ESP32-S3; its lib docs contrast polled HAL subscriptions with persistent raw-FFI interrupts ([ADR-005](docs/adr/005-raw-ffi-persistent-interrupts.md), [ADR-006](docs/adr/006-interrupt-encoder-instance-and-api-shape.md)).
- ESP32-C3 example twins on both tiers (`b3f`, `poti`, `poti_led`, `hall_linear`, `hall_switch`, `i2c_scan`, `passive_buzzer_arpeggio`, `active_buzzer_morse`, `blink`), `idf_s3_rotary`, and the `build-example` / `run` / `check-hal` / `clean-hal` / `clean-idf` recipes ([ADR-004](docs/adr/004-i2c-bus-pattern.md)).
- `maintenance-plan.md`: the audit / plan / maintenance protocol for dependency waves, mirroring the sibling repos.

### Changed
- **Compatibility — building the two hardware tiers now requires Rust 1.95+** (was 1.88), the floor declared by `esp-hal 1.2` / `esp-bootloader-esp-idf 0.6` / `esp-println 0.18`. `tamer` and every host gate stay on 1.88, so pure-core consumers are unaffected; only device builds need the newer toolchain. Cargo compares `rust-version` numerically and ignores a `-nightly` suffix, so an older nightly passes the gate and then fails to compile.
- Dependency wave (2026-09-25): `esp-hal` `=1.2.2` / `esp-bootloader-esp-idf` `=0.6.0` / `esp-println` `=0.18.0` and `esp-idf-hal` `=0.47.0` / `esp-idf-svc` `=0.53.0` / `esp-idf-sys` `=0.38.1` (0.38.0 is yanked upstream) / `embuild` `=0.33.5`; no API migration needed, `critical-section` stays `=1.2.0` (backend source-identical), compile-verified on C3 / C6 / ESP32 / S3, hardware validation pending.
- Toolchain and CI: RISC-V bare-metal builds run on stable with the prebuilt rustup target instead of a floating nightly (`-Zbuild-std` is only needed for Xtensa) and `scripts/build-example.sh` is bash-3.2-safe; the `deny.toml` `paste` ignore is re-cited to `esp-hal 1.2.2` (review 2026-12-31); GitHub Actions moved to node24 tags (`checkout@v5`, `setup-just@v4`).
