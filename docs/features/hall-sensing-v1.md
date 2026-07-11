# Feature: Hall-effect Sensing (linear analog + digital switch) v1

## Decisions
|                                                                                                                                   Decision | Reason                                                                                                               | Rejected Alternative                                                                   |
|-------------------------------------------------------------------------------------------------------------------------------------------:|:---------------------------------------------------------------------------------------------------------------------|:---------------------------------------------------------------------------------------|
|                 Two Hall sensing paths: `tamer::hall` (linear analog via ADC + deviation threshold) and `tamer::presence` (digital switch) | Real KY-003 modules in the field mix A3144 digital switches (one-pole, Schmitt-trigger) and 49E/AH477 linear sensors | Single "Hall" abstraction; treat all sensors the same                                  |
|                                    Linear Hall model: calibrate no-magnet midpoint at startup, then emit `Presence` on threshold deviation | Captures both poles symmetrically; works with `SlidingAverage` for noise-immune operation                            | Fixed midpoint or per-sensor calibration file                                          |
|                                     Digital Hall path: read KY-003 / A3144 via `DigitalPresence` (ActiveLow, debounced) like a reed switch | Unipolar open-collector logic maps directly to digital debounce; no ADC needed                                       | Squeeze digital switch into the linear ADC model (causes clipping / wrong abstraction) |

## Constraints
- `no_std`, MSRV 1.88; pure core has zero hardware dependencies (no ADC driver baked in).
- `tamer::hall` is HAL-agnostic: consumers feed it raw ADC samples and manage calibration timing.
- Hardware examples use `embedded_hal::adc::OneShot` / `adc::Adc` adapters (esp-hal tier) or ESP-IDF ADC APIs (idf tier); all smoothing and threshold logic stays host-testable in `tamer`.
- Marketplace labels ("KY-003", "Hall sensor") are ambiguous; always verify the physical TO-92 chip marking (A3144 vs SS49E / AH477) before selecting the sensing path.

## Open Questions
- [ ] `idf_c3_hall_switch` twin — deliberately deferred: the digital-switch example ships on the esp-hal tier only (that is the board physically on hand); the linear path keeps both tier twins. Add the ESP-IDF switch twin if/when a `std`-tier consumer needs it. This asymmetry is intentional, not an oversight.
- [ ] Add non-ESP32 chip examples (ARM STM32, RISC-V beyond ESP32-C3/C6)?
- [ ] Interrupt-driven ADC sampling for the linear path (vs. polled `OneShot`)?
- [ ] Latch-based Hall sensors (AH3572, etc.) that remember magnet approach direction — separate module or variant?
- [ ] Hysteresis (separate rising/falling thresholds) for `HallSensor` to prevent boundary chatter in noisy production use — v1 uses a single absolute-deviation threshold plus `SlidingAverage`; revisit for v2.
- [ ] Shared calibration / ADC-read-failure helper for the linear examples — `hal_c3_hall_linear` and `idf_c3_hall_linear` duplicate that logic, but they live in different crates with incompatible ADC APIs (`esp-hal` `nb::block!` vs `esp-idf-hal` `read_raw`), so a clean cross-tier helper is non-trivial; deferred.

## State
- [x] Design approved (linear + digital paths; startup calibration; `project-lore` records the KY-003 / A3144 pitfall)
- [x] Core implementation (`tamer::hall::HallSensor` + `tamer::smoothing::SlidingAverage`)
- [x] Linear ADC examples (`hal_c3_hall_linear`, `idf_c3_hall_linear`)
- [x] Digital switch example (`hal_c3_hall_switch` on top of `tamer::presence`)
- [x] Host/unit tests passing (`just verify` green: 102 unit + 11 doctests)
- [x] `hal_c3_hall_switch` hardware-verified (KY-003/A3144 module on ESP32-C3: clean debounced transitions on triggering pole, no response on opposite)
- [ ] Linear examples (`hal_c3_hall_linear`, `idf_c3_hall_linear`) hardware-verified (compile clean; 49E sensor not yet on hand)
- [x] Documentation updated (CHANGELOG, `project-lore` KY-003/A3144 entry, this feature doc)

## Session Log
- 2026-07-10 — Linear analog `tamer::hall` + `tamer::smoothing` landed; examples `hal_c3_hall_linear` and `idf_c3_hall_linear` use ADC1 + calibration + deviation threshold.
- 2026-07-10 — Discovered KY-003 test board module is A3144 (unipolar digital switch), not a linear sensor; renamed examples `*_hall_sensor` → `*_hall_linear` to distinguish the sensing model; added `hal_c3_hall_switch` example reading the A3144 via `tamer::presence::DigitalPresence`.
- 2026-07-10 — Added `project-lore` entry: chip markings, symptom (sensor idles at ADC max for A3144), and the two-path fix (linear ADC vs. digital debounce).
- 2026-07-10 — Added `HallSensor::set_threshold` (symmetry with `set_midpoint`) + typical-workflow unit test; reordered `lib.rs` so `hall`/`smoothing` present as one coherent addition; documented single-threshold (no-hysteresis) rationale for v1; confirmed `hal_c3_hall_switch` on real hardware; replaced ambiguous "Tests passing" checkbox with three granular items tracking host/unit tests (passed), digital-switch hardware verification (passed), and linear examples (pending hardware); added two open questions (hysteresis for production, cross-tier calibration helper).
