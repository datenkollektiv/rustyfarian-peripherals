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
|                                            `b3f` smoke per chip per tier (`idf_{esp,c6,s3}_b3f`) | Tier parity without multiplying examples                                           | Port feature examples to all chips                            |
|                                                    Feature examples stay on their reference chip | `tamer` logic is host-tested once, not per chip                                    | Per-chip feature examples                                     |
|             AVR covers `tamer` only, setup copied from ws2812 (`nightly-2025-04-27`, `avr-none`) | VISION lists other MCU families as a non-goal; ws2812 setup is proven              | AVR tier crate (revisit with ws2812 merge)                    |
|                           Excluded `examples/avr-nano-tamer` package, building `default` + `hal` | A lib `check` never links; smallest useful surface, float features later           | `cargo check -p tamer` only; `tilt` / `bme680` size probe now |
| CI like ws2812: path-filtered `cross-target-{riscv,xtensa,avr}`, plus weekly AVR upstream canary | Per-PR gate for esp-hal chips and AVR link; the pinned rev hides upstream breakage | Local-only matrix; no canary                                  |
|                                                   ESP-IDF chips stay in the local `check-matrix` | ws2812 has no IDF CI job; esp-idf-sys is heavy                                     | IDF jobs per PR                                               |
|                 Rotary examples per chip: `hal_{esp,c3,c6}_rotary_isr`, `idf_{esp,c3,c6}_rotary` | The ISR is the chip-sensitive code                                                 | Rotary on S3 only                                             |
|                     A chip x tier cell is covered only by its own bench run; build results never tick it | Build and link evidence is not hardware evidence; C3 does not stand in for C6      | Inherit coverage across chips or tiers                        |

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

## Hardware pass

A cell reads `pass (date, commit, board)` only once its own bench run has passed, and the commit is recorded with it.
The bench runs are guided by `just bench <board> <check>` (see [`bench/README.md`](../../bench/README.md)); each tick is recorded in [`release-plan.md`](../../release-plan.md) § Final hardware pass.

| Chip                         | Tier | `b3f`                                 | `rotary`                 | Other scenarios                                        |
|:-----------------------------|:----:|:--------------------------------------|:-------------------------|:-------------------------------------------------------|
| ESP32 WROOM-32 / Feather V2  | hal  | pass (2026-11-12, 6214033, ESP32 WROOM-32) | pass (2026-11-12, 6214033, ESP32 WROOM-32) | `oled-ssd1306` pass (2026-11-12, 6214033, ESP32 WROOM-32); `oled-sh1106` not run |
| ESP32 WROOM-32 / Feather V2  | idf  | pass (2026-11-12, 6214033, ESP32 WROOM-32) | pass (2026-11-12, 6214033, ESP32 WROOM-32) | `oled-ssd1306` pass (2026-11-12, 6214033, ESP32 WROOM-32); `oled-sh1106` not run |
| ESP32-C3 SuperMini           | hal  | not run                               | not run                  | `hall-linear`, `poti` pass (2026-11-08, 9430069, C3 SuperMini); `oled` pass (2026-11-08, 2d04fbd, C3 bench rig); others not run |
| ESP32-C3 SuperMini           | idf  | not run                               | not run                  | `hall-linear`, `poti` pass (2026-11-08, 9430069, C3 SuperMini); `oled` pass (2026-11-08, 2d04fbd, C3 bench rig); others not run |
| ESP32-C6-DevKitC-1           | hal  | not run                               | not run                  | n/a (no scenario written)                              |
| ESP32-C6-DevKitC-1           | idf  | not run                               | not run                  | n/a (no scenario written)                              |
| CrowPanel 1.28" (S3)         | hal  | pass (2026-11-11, c8907e9, CrowPanel S3) | `rotary`, `rotary-isr` pass (2026-11-11, c8907e9, CrowPanel S3) | n/a (no scenario written)                    |
| CrowPanel 1.28" (S3)         | idf  | pass (2026-11-11, c8907e9, CrowPanel S3) | pass (2026-11-11, c8907e9, CrowPanel S3) | n/a (`rotary-isr` is hal-only)                         |

The S3 row is complete: every S3 gate passed unattended and attended on one commit.

## Known tier gaps

- `hal_c3_hall_switch` has no ESP-IDF twin by decision; see the Decisions table in [`docs/features/archive/hall-sensing-v1.md`](archive/hall-sensing-v1.md).
- `hal_c6_passive_buzzer_arpeggio` is a C6 LEDC pitch probe, suspected one octave high; feature examples stay on their reference chip, and the probe is optional in `release-plan.md`.
- `hal_c3_bme680_async` and `hal_s3_rotary` exist only on esp-hal (async), so they have no ESP-IDF twin to compare against.
- `hal_c3_rig_check` is a hal-only diagnostic for the C3 bench rig; it has no ESP-IDF twin by design.
- `oled` (I2C OLED, [signwriter v1](signwriter-v1.md)) is the scenario for every chip with an I2C connector: C3 is `pass` on both tiers, C6 and the ESP32 WROOM-32 are planned for stage 3, the Feather V2 profile is deferred to the next release (no STEMMA QT cable), and the S3 is `n/a (touch bus, no connector)`, because its I2C 6/7 is the touch bus.
- The OLED examples take their I2C pins from a board profile ([ADR-014](../adr/014-board-profiles.md)): C3 SuperMini, C6-DevKitC-1, ESP32 WROOM-32 and Feather V2 (`just build-example <ex> feather-v2`); the bench runs `oled` once per board, so a WROOM-32 pass does not count for the Feather V2.

## Rotary pins (A / B / button, internal pull-ups, active-low)
| Board                       | A  | B  | Button | Why                                                                                    |
|:----------------------------|:--:|:--:|:------:|:---------------------------------------------------------------------------------------|
| ESP32 WROOM-32 / Feather V2 | 32 | 33 |   27   | A/B in bank 1, button in bank 0; 34–39 lack pull-ups; check the Feather V2 pinout card |
| ESP32-C3 SuperMini          | 0  | 1  |   5    | Click on 5 (wake-capable); 10 is the buzzer; 2/8/9 strapping, 18–21 USB/UART                   |
| ESP32-C6-DevKitC-1          | 18 | 19 |   20   | Adjacent on J3; clear of strapping, USB, UART                                          |
| CrowPanel 1.28" (S3)        | 45 | 42 |   41   | Board-wired encoder, unchanged                                                         |

## Constraints
- `tamer` `rust-version` stays at or below what the AVR nightly reports (`1.88.0-nightly`).
- `-Z build-std=core` goes on the command line, because the root `.cargo/config.toml` `build-std` arrays merge.
- avr-gcc is required even for a core-only AVR build (libgcc multiply intrinsics).
- Xtensa builds use `cargo +esp`; RISC-V esp-hal builds use stable.

## State
- [x] Design approved
- [x] `check-matrix`, `build-matrix` (link proof), `check-avr-target`, `setup-avr`, `build-avr-example` (and `-upstream`) recipes
- [x] `idf_{esp,c6,s3}_b3f`, the six rotary examples, and `examples/avr-nano-tamer`
- [x] `cross-target-{riscv,xtensa,avr,avr-upstream}` workflows (written; first GitHub run pending)
- [ ] Hardware pass: S3 `b3f`, `rotary` and `rotary-isr` on both tiers (2026-11-11), ESP32 WROOM-32 `b3f`, `rotary` and `oled-ssd1306` on both tiers (2026-11-12) and C3 `hall-linear`, `poti` and `oled` on both tiers (2026-11-08) passed; every other cell is `not run`
- [x] Documentation updated (AGENTS.md matrix, hardware-setup, CHANGELOG)

## Session Log
- 2026-11-04 — Feature doc created via /feature from the chip-matrix and ws2812 AVR review.
- 2026-11-04 — Updated: CI like ws2812, AVR `default` + `hal`, rotary pins chosen for ESP32/C3/C6.
- 2026-11-04 — Infra, examples and AVR landed; `just verify` blocked by an out-of-scope fmt diff; docs updated, hardware pass open.
- 2026-11-07 — Split the compile and link evidence from the hardware pass table, added the known tier gaps and the decision that build results never tick a cell, and recorded the 2026-11-06 hal S3 `b3f` pass (commit to be filled in).
- 2026-11-11 — S3 row passed on c8907e9 (unattended + attended, both tiers); `rotary-isr` fast spin is clean, a flat-out spin skips a few edges from contact bounce.
- 2026-11-12 — ESP32 WROOM-32 `b3f`, `rotary` and `oled-ssd1306` passed on 6214033 (both modes, both tiers); OLED split per controller, `oled-sh1106` waits for a module.
