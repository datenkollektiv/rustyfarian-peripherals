---
gate: hardware
gate-note: bench run on the ESP32 (WROOM-32 / Feather V2), C3, C6 and S3 boards
desk-work: available
---

# Feature: Chip matrix v1

Every tier compiles, lints and links on all four ESP32 chips, `tamer` links on AVR, and chip-sensitive behaviour is checked on the bench before the ADR-010 release.

## Decisions
|                                                                                         Decision | Reason                                                                             | Rejected Alternative                                          |
|-------------------------------------------------------------------------------------------------:|:-----------------------------------------------------------------------------------|:--------------------------------------------------------------|
|                                      Check + clippy the libs on ESP32, C3, C6, S3 for both tiers | IDF single/dual-core rotary paths compile for C3 only today                        | Every example on every chip                                   |
|                                                   `just check-matrix`, kept out of `just verify` | Four esp-idf-sys builds are slow                                                   | Fold into `verify`                                            |
|                                                        One chip at a time; stop below 4 GiB free | A full RAM disk is lore; auto-prune would delete other repos                       | Auto-prune between chips                                      |
|                                            `b3f` smoke per chip per tier (`idf_{esp32,c6,s3}_b3f`) | Tier parity without multiplying examples                                           | Port feature examples to all chips                            |
|                                                    Feature examples stay on their reference chip | `tamer` logic is host-tested once, not per chip                                    | Per-chip feature examples                                     |
|             AVR covers `tamer` only; setup from ws2812 (`nightly-2025-04-27`, `avr-none`) | VISION lists other MCU families as a non-goal; ws2812 setup is proven              | AVR tier crate (revisit with ws2812 merge)                    |
|                           Excluded `examples/avr-nano-tamer` package, building `default` + `hal` | A lib `check` never links; smallest useful surface, float features later           | `cargo check -p tamer` only; `tilt` / `bme680` size probe now |
| CI like ws2812: path-filtered `cross-target-{riscv,xtensa,avr}` | Per-PR gate for esp-hal chips and AVR link | Local-only matrix |
| Weekly AVR upstream canary and an `idf` job | The pinned rev hides upstream breakage | No canary |
|                                    ESP-IDF runs in CI as one `cross-target-idf` cell per chip | One cell per chip keeps cold esp-idf-sys builds within the timeout (see note) | One IDF job for all four chips; local-only `check-matrix` |
|                 Rotary examples per chip: `hal_{esp32,c3,c6}_rotary_isr`, `idf_{esp32,c3,c6}_rotary` | The ISR is the chip-sensitive code                                                 | Rotary on S3 only                                             |
|                     A chip x tier cell is covered only by its own bench run; builds never tick it | Build and link evidence is not hardware evidence; C3 does not stand in for C6      | Inherit coverage across chips or tiers                        |

Note: the ESP-IDF tier was never built in CI before `cross-target-idf`.

## Compile and link evidence

Recorded 2026-11-05: `just check-matrix` (check and clippy `-D warnings`) and `just build-matrix` (link proof of `hal_*_b3f` and `idf_*_b3f`) ran green on all eight cells.
These rows show build status only and never tick a hardware cell.

| Chip            | hal check + clippy | idf check + clippy | hal build-matrix (link) | idf build-matrix (link) |
|:----------------|:------------------:|:------------------:|:-----------------------:|:-----------------------:|
| ESP32           | pass               | pass               | pass                    | pass                    |
| ESP32-C3        | pass               | pass               | pass                    | pass                    |
| ESP32-C6        | pass               | pass               | pass                    | pass                    |
| ESP32-S3        | pass               | pass               | pass                    | pass                    |

The AVR link of `tamer` (`just check-avr-target`, `just build-avr-example`) is not part of this table.

## Known tier gaps

- `hal_c3_hall_switch` has no ESP-IDF twin by decision.
- `hal_c6_passive_buzzer_arpeggio` is a C6 LEDC pitch probe, suspected one octave high; feature examples stay on their reference chip, and the probe is optional in `release-plan.md`.
- `hal_c3_bme680_async` and `hal_s3_rotary` exist only on esp-hal (async), so they have no ESP-IDF twin to compare against.
- `hal_c3_rig_check` is a hal-only diagnostic for the C3 bench rig; it has no ESP-IDF twin by design.
- The S3 has no OLED check: its I2C 6/7 is the touch bus.
- The OLED examples take their I2C pins from a board profile ([ADR-014](../adr/014-board-profiles.md)), so each profile is its own bench board.

## Constraints
- `tamer` `rust-version` stays at or below what the AVR nightly reports (`1.88.0-nightly`).
- `-Z build-std=core` goes on the command line, because the root `.cargo/config.toml` `build-std` arrays merge.
- avr-gcc is required even for a core-only AVR build (libgcc multiply intrinsics).
- Xtensa builds use `cargo +esp`; RISC-V esp-hal builds use stable.

## State
- [x] Design approved
- [x] `check-matrix`, `build-matrix` (link proof), `check-avr-target`, `setup avr`, `build-avr-example` (and `-upstream`) recipes
- [x] `idf_{esp32,c6,s3}_b3f`, the six rotary examples, and `examples/avr-nano-tamer`
- [x] `cross-target-{riscv,xtensa,avr,avr-upstream}` workflows (written; first GitHub run pending)
- [x] `cross-target-idf` matrix (`just ci-idf <chip>` for esp32, c3, c6, s3; written; first GitHub run pending)
- [ ] Hardware pass: [`release-plan.md`](../../release-plan.md) § Final hardware pass
- [x] Documentation updated (AGENTS.md matrix, hardware-setup, CHANGELOG)
