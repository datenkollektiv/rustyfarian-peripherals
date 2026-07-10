# Feature: Analog Range Map (clamped linear remap) v1

A pure, host-testable clamped linear transfer function that maps an analog reading
(e.g. LDR raw ADC counts) onto an output value (e.g. LEDC PWM duty). It provides the
host-testable mapping step for analog→PWM auto-adjust behavior — such as dimming a
backlight from ambient light, after the raw reading has been smoothed with
`smoothing::SlidingAverage` — so that logic lives in the pure core rather than as
hand-rolled inline math in a device example.

## Background

Rationale for *where* and *what to call it*, kept out of the design contract below:

- **Why this repo (not `ws2812` / a new output crate).** `RangeMap` produces an
  output value, but it belongs here: `VISION.md` broadened scope from input-only to
  **all** peripherals — input *and* output — and the type is pure math that imports
  nothing (in particular it never touches `rustyfarian-ws2812`). The analog input
  side it pairs with (`analog`, `smoothing`) already lives here.
- **Why the generic name.** The first consumer is an ambient-light→backlight
  auto-dim, but the mechanism is a generic clamped range remap. `BrightnessCurve`
  would specialize on the first application; the crate names the *mechanism*
  (`SlidingAverage`, `AnalogRange`) and shows the application in a doctest. `tamer`
  has no `input::`/`output::` namespace — modules are flat top-level — so the type
  lives in a flat `range_map` module.

## Decisions
|                                                                                           Decision | Reason                                                                                                                                         | Rejected Alternative                                                                        |
|---------------------------------------------------------------------------------------------------:|:-----------------------------------------------------------------------------------------------------------------------------------------------|:--------------------------------------------------------------------------------------------|
|                    Add a pure `RangeMap` — clamped linear `u16 → u8` remap with optional inversion | Broadly reusable analog→PWM mapping (LDR→backlight, pot→LED, temp→fan); keeps each consumer's logic host-testable per the pure-core discipline | Hand-roll the map inline in each demo — not testable, duplicated per consumer               |
|                                        Generic `RangeMap` type in a flat `tamer::range_map` module | Matches the crate's mechanism-named, flat-module idiom (see Background)                                                                        | `BrightnessCurve` / an `input::`/`output::` namespace                                       |
|                             `u16 → u8` (raw 12-bit ADC → 8-bit LEDC duty), `u8`-only output for v1 | Matches the ADC input and LEDC duty widths; a wider output is a non-breaking addition once a consumer needs it (see Deferred)                  | Generic / const-generic output width now — speculative, contradicts demand-driven           |
|          Round-to-nearest scaling, widened intermediates — the exact `AnalogRange::normalize` rule | One canonical rounding rule across the crate; `normalize` / `percent` already add `span / 2` before dividing in `u32`                          | Floor-based integer division (biases the whole curve low) or a bespoke rounding rule        |
|     Guard `in_min == in_max` with a panicking `assert!` in the `const fn` constructor (`# Panics`) | Matches `AnalogRange::new` exactly — the crate's idiom for range-construction invariants (fires in release, is `const fn`)                     | `debug_assert!` only, or a `Result`-returning constructor — no other range type uses either |

## Behavioral contract

The mapping the implementation and its tests must satisfy. Let
`in_span = in_max − in_min` (≥ 1, guaranteed by the constructor).

- **Clamp, never extrapolate.** `map(reading)` first clamps `reading` to
  `in_min..=in_max` (as `AnalogRange::clamp` does); readings outside the input range
  saturate at an endpoint.
- **Linear scale, round-to-nearest.** For an in-range reading:
  `out = out_min + round_nearest((reading − in_min) · (out_max − out_min) / in_span)`,
  where `round_nearest` adds `in_span / 2` before the integer division — the exact
  rule `AnalogRange::normalize` and `AnalogSample::percent` already use. Intermediates
  widen (to `u32`/`i32`) so no step overflows.
- **Endpoints are exact equalities.** `map(≤ in_min) == out_min` and
  `map(≥ in_max) == out_max` exactly — the rounding term vanishes at the endpoints.
  Guaranteed, not approximate.
- **Monotonic.** `map` is non-decreasing in `reading` (non-increasing when inverted).
- **Inversion — canonical definition.** `inverted()` **swaps the output endpoints**:
  `map(in_min) == out_max` and `map(in_max) == out_min`. Equivalently it reverses the
  normalized position (`p ↦ 1 − p`); the two coincide for a linear map, and the
  swap-endpoints form is the normative one that tests assert. `inverted().inverted()`
  restores the original mapping. Descending output bounds (`out_min > out_max`) are
  also supported directly via the signed output span — the same falling map without
  `inverted()` — though `inverted()` is the preferred, more readable form for an
  ascending range.
- **`map()` is total.** For **any successfully constructed `RangeMap`**, `map()`
  cannot panic for any `u16` reading. The single panic path is construction
  (`in_min == in_max`).

## Constraints
- Pure `no_std`, no-alloc, host-testable; **no hardware/HAL coupling** — consistent with `analog` / `smoothing` / `hall`. MSRV 1.88.
- `const fn` constructors where feasible (`new`, `inverted`); construction is the only panic path (see the totality guarantee above).
- Pure value type (like `SlidingAverage` / `AnalogRange` / `HallSensor`) — no hardware trait, therefore no `Noop*` mock required (that rule applies to hardware-interaction traits).
- Never imports `rustyfarian-ws2812` — the hard `VISION.md` boundary.

## Required tests

The contract the implementation PR must satisfy (host tests):

- **Endpoints exact:** `map(in_min) == out_min`, `map(in_max) == out_max`.
- **Clamping:** readings below `in_min` → `out_min`, above `in_max` → `out_max`; no extrapolation.
- **Rounding:** an interior value where floor and nearest differ rounds to nearest per the `AnalogRange::normalize` rule.
- **Monotonicity:** non-decreasing output across a swept input range.
- **Inversion symmetry:** `inverted()` swaps endpoints (`map(in_min) == out_max`, `map(in_max) == out_min`); `inverted().inverted()` restores the original mapping.
- **Construction guard:** `in_min == in_max` panics (documented under `# Panics`).
- **Totality:** no `u16` reading panics for a successfully constructed map (exhaustive/fuzz sweep over a couple of ranges).
- **Descending / asymmetric:** `out_min > out_max` yields a monotone-decreasing map without `inverted()`; a non-zero input range with `inverted()` and asymmetric outputs maps a non-midpoint reading to the endpoint-swapped value.

## Deferred (explicitly decided — not open)
- **Gamma / perceptual shaping → v2.** A separate non-linear transfer function, not a parameter of a linear remap; folding it in now blurs `RangeMap`'s single responsibility. Matches the `HallSensor` hysteresis-deferral precedent.
- **Higher-resolution (`u16`) output → later, additively.** v1 is `u8` (8-bit LEDC duty); a wider output is a non-breaking addition when a consumer needs it.
- **Deadband / hysteresis near the endpoints → not planned.** Input smoothing is handled upstream (`SlidingAverage`); revisit only if field flicker actually appears.

## Open Questions
- [x] **In-repo example?** Shipped the `hal_c3_poti_led` / `idf_c3_poti_led` twin (pot → `RangeMap` → LEDC PWM LED on GPIO6, external LED on the C3-DevKitM-1). First output/PWM examples in the repo. Chose LEDC PWM over the on-board addressable RGB, which is WS2812 → the sibling `ws2812` repo's domain.
- [x] **Docs-sync** — resolved by spinning out: `README.md` / `AGENTS.md` still frame the repo as "input peripherals" (stale vs `VISION.md`'s input+output scope), but that is a cross-cutting docs task, not a `RangeMap` deliverable, so it now lives on the roadmap (Near term) as a standalone follow-up.

## State
- [x] Design approved (fit confirmed against `VISION.md`; generic `RangeMap` naming; behavioral contract specified)
- [x] Core implementation (`tamer::range_map::RangeMap`)
- [x] Host tests passing (per **Required tests** above)
- [x] Documentation updated (module docs, `prelude` export, LDR→backlight doctest, CHANGELOG)
- [x] In-repo example — `hal_c3_poti_led` / `idf_c3_poti_led` twin (pot → `RangeMap` → LEDC PWM LED, GPIO6). Both tiers device-compiled and **hardware-verified on an ESP32-C3-DevKitM-1** (full dark → full bright after the startup calibration sweep). Needed an `AnalogCalibration` sweep so the map's endpoints track each stack's real ADC span — see `project-lore` (esp-hal vs esp-idf ADC raw floor).

## Session Log
- 2026-07-10 — Feature doc created via `/feature`. Assessed fit with `rust-engineer`: fits per `VISION.md`'s input+output scope (`RangeMap` imports nothing, no `ws2812` coupling). Chose generic `RangeMap` in flat `tamer::range_map` over `BrightnessCurve` / an `output::` namespace. Resolved the technical open questions with precedent-driven answers — `u8`-only for v1, gamma → v2, `in_min == in_max` via panicking `assert!` matching `AnalogRange::new`.
- 2026-07-10 — Restructured into a stricter implementation spec after PR-review feedback: pulled repo-fit/naming rationale into **Background**; added a **Behavioral contract** (clamp-then-scale, round-to-nearest matching `AnalogRange::normalize`, exact endpoints, canonical inversion = swap output endpoints, `map()` total post-construction); promoted the test list into **Required tests**; split explicitly-decided deferrals (gamma → v2, `u16` output, deadband) into a **Deferred** section so **Open Questions** holds only genuinely-open items (in-repo example, docs-sync). Reworded "panic-free at `map()`" to "`map()` is total for any successfully constructed `RangeMap`".
- 2026-07-10 — Added the `hal_c3_poti_led` / `idf_c3_poti_led` twin (pot on ADC1/GPIO4 → `RangeMap::new(0, 4095, 0, 255)` → 8-bit LEDC PWM on GPIO6, external active-high LED) via a parallel implement→independently-verify workflow (two `embedded-systems` agents + a fresh `code-reviewer`). Repo's first output/PWM examples. GPIO6 chosen as a non-strapping / non-WS2812 / non-USB/UART/flash/ADC pin; on-board addressable RGB deliberately avoided (WS2812 = `ws2812` sibling's domain). hal example device-compiled **and linked** for `riscv32imc`; idf example review-verified against pinned `esp-idf-hal 0.46.2` source but its full compile was initially blocked by a 100%-full shared build RAM disk (`/Volumes/RustBuilds`), not a code defect. `tamer` untouched; host `just verify` still green.
- 2026-07-10 — Hardware bug fixed + verified: the twin's LED never reached full dark/bright on esp-hal because a hard-coded `RangeMap::new(0, 4095, …)` never hit the raw ADC's real endpoints (esp-hal's uncalibrated `read_oneshot` floors ~100–200 counts above 0; esp-idf's calibrated driver reached the rails — a tier discrepancy). Fixed both examples to run an `AnalogCalibration` startup sweep (mirroring `hal_c3_poti`/`idf_c3_poti`) and build the map from the calibrated range. Both tiers now compile (idf too, RAM disk had space) and were **hardware-verified on an ESP32-C3-DevKitM-1** — full dark → full bright. Logged the ADC-floor gotcha to `docs/project-lore.md`.
- 2026-07-10 — Second-pass PR review + roadmap hygiene: confirmed the doc's hardware-verification status is internally consistent (State + Session Log both say hardware-verified). Documented that descending output bounds (`out_min > out_max`) are supported directly (prefer `inverted()` for readability) and locked it in with two tests (`descending_output_bounds_without_inverted`, `inverted_asymmetric_non_zero_range`). Removed `RangeMap` from the roadmap (already in CHANGELOG `[Unreleased]`) and spun the docs-sync open question out to the roadmap (Near term). All State boxes ticked and no open questions remain → archived to `docs/features/archive/`.
- 2026-07-10 — Implemented via an implement→independently-verify workflow (`rust-engineer` + a fresh `code-reviewer`). `tamer::range_map::RangeMap` landed in `crates/tamer/src/range_map.rs`, re-exported at the crate root and in `prelude`. Rounding mirrors `AnalogRange::normalize`; a signed `out_span` (i32) keeps `map()` total for inversion and `out_min > out_max` alike. 10 unit tests + a module doctest cover the Required-tests contract; the independent verifier additionally ran an exhaustive 65 536-reading sweep across 8 configs (zero panics) and confirmed `just verify` green (114 unit + 12 doctests, clippy `-D warnings` clean). No fix iterations. CHANGELOG updated under `## [Unreleased]`. Remaining open: in-repo hardware example and the README/AGENTS docs-sync.
