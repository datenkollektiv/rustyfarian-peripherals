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

## Hardware pass
| Behaviour (both tiers)                            | ESP32 | C3 | C6 | S3 |
|:--------------------------------------------------|:-----:|:--:|:--:|:--:|
| `b3f` smoke: GPIO input + console                 |  ✓   | ✓ | ✓ | ✓ |
| Rotary ISR decode, `edge_count` / `skipped_count` |  ✓   | ✓ | ✓ | ✓ |
| LEDC buzzer pitch                                 |       | ✓ | ✓ |    |
| I2C sensors, ADC (`release-plan.md` list)         |       | ✓ |    |    |

## Rotary pins (A / B / button, internal pull-ups, active-low)
| Board                       | A  | B  | Button | Why                                                                                    |
|:----------------------------|:--:|:--:|:------:|:---------------------------------------------------------------------------------------|
| ESP32 WROOM-32 / Feather V2 | 32 | 33 |   27   | A/B in bank 1, button in bank 0; 34–39 lack pull-ups; check the Feather V2 pinout card |
| ESP32-C3 SuperMini          | 0  | 1  |   10   | 4–7 used, 2/8/9 strapping, 18–21 USB/UART                                              |
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
- [ ] Hardware pass on the bench boards
- [x] Documentation updated (AGENTS.md matrix, hardware-setup, CHANGELOG)

## Session Log
- 2026-11-04 — Feature doc created via /feature from the chip-matrix and ws2812 AVR review.
- 2026-11-04 — Updated: CI like ws2812, AVR `default` + `hal`, rotary pins chosen for ESP32/C3/C6.
- 2026-11-04 — Infra, examples and AVR landed; `just verify` blocked by an out-of-scope fmt diff; docs updated, hardware pass open.
