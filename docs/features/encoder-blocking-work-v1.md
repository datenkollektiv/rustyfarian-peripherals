---
gate: none
desk-work: available
---

# Feature: esp-hal Encoder Under Blocking Work v1

The async esp-hal `rotary::Encoder` decodes in task context, so blocking work between two `wait_for_step` calls drops detents.
Measured on the CrowPanel 1.28" S3 at `dfb3f45` (30 detents/rev): lossless up to ~13 ms of work per step, ~50–63 % of detents lost under a ~90 ms blocking SPI redraw.
A rotation-only task on the same cooperative executor is still stalled, so `split()` alone does not fix it.

**Phase 1 — make starvation visible:** state the measured budget in rustdoc, and count 2-bit jumps in `tamer::rotary` so consumers can detect lost steps.
**Phase 2 — make decoding survive the redraw:** spike both candidates below, pick one on hardware, record it as an ADR-006 amendment.

## Decisions
|                                                 Decision | Reason                                                              | Rejected Alternative                                                          |
|---------------------------------------------------------:|:--------------------------------------------------------------------|:------------------------------------------------------------------------------|
|                                      One doc, two phases | One consumer report, one acceptance criterion                       | Separate `tamer` counter feature                                              |
|               Lost-step counter lives in `tamer::rotary` | Pure Gray-code logic, host-testable                                 | Counting in the esp-hal tier                                                  |
|        Spike both isolation candidates before committing | Hardware decides; both have real costs                              | Adopting the request's `split()` plan as-is                                   |
| Existing `Encoder` API stays usable for simple consumers | Request acceptance; ADR-006 shape is validated                      | Replacing `wait_for_step`                                                     |
|                                       Candidate B over A | Lossless under the 90 ms redraw; tier parity with the IDF `Encoder` | Candidate A: `InterruptExecutor` plus `split()` (forces embassy on consumers) |

## Constraints
- Phase-2 candidate A: `Encoder::split()` plus the rotation half on an esp-rtos interrupt-priority executor, shown in `hal_s3_rotary_isolated`.
- Phase-2 candidate B: decode inside a GPIO interrupt handler, mirroring the IDF tier, at the cost of statics and `critical_section`.
- The counter must not change `QuadratureDecoder::update` results; a 2-bit jump still yields `None`.
- The counter must stay compatible with the arithmetic decode planned in [IRAM-safe ISR v1](iram-safe-isr-v1.md).
- The shared encoder trait (ADR-006) stays the synchronous core; phase 2 must not force an async trait.
- Tests cover CW and CCW symmetrically.

## Open Questions
- [x] Candidate A or B — which one gets 30/30 detents under the ~90 ms redraw? — both; B chosen for tier parity with the IDF `Encoder` (see Spike results).
- [x] Counter API shape: monotonic `skipped_count()` vs. take-and-clear? — monotonic, wrapping `u32`, untouched by `reset`/`set_position`; matches `edge_count`/`isr_count`, safe for multiple readers.
- [x] Does a 2-bit jump from contact bounce inflate the counter enough to make it noisy at speed? — no; 0 skips at fast speed when decoding keeps up, so a rising count means starvation.
- [x] Land phase 2 before, with, or after the shared encoder trait extraction? — before; the trait extraction is unchanged in shape and simpler, since rotation maps to `position()` on both tiers.

## Follow-ups
- IDF `Encoder::skipped_count` for diagnostic parity with the esp-hal tier (out of scope for phase 2).

## State
- [x] Design approved
- [x] Phase 1: rustdoc states the measured ~10 ms per-step budget
- [x] Phase 1: `tamer::rotary` lost-step counter with host tests
- [x] Phase 2: spike A and B on the CrowPanel load test
- [x] Phase 2: ADR-006 amendment records the choice
- [x] Phase 2: implementation + example; 30/30 detents with a ~90 ms redraw
- [x] CHANGELOG, README
- [x] Final validation: downstream knob load test with the real GC9A01 redraw on the pushed rev

## Spike results (2026-10-27)
CrowPanel 1.28" S3, 30 detents/rev, 90 ms blocking busy-wait per position change, round trips to a marked position.
A window covers motion until 1.5 s idle; round trips merged into one window, so pass = cw = ccw ≥ 30 per trip and delta 0.

| Candidate                       | Normal ×2 | Fast ×2 | Reversals | Net drift | Skipped |
|:--------------------------------|:----------|:--------|:----------|:----------|--------:|
| Baseline (cooperative executor) | 16/15     | 12/16   | 17/17     | −3        |      30 |
| A: `InterruptExecutor` prio 3   | 60/60     | 60/60   | 37/37     | 0         |       0 |
| B: decode in GPIO ISR           | 60/60     | 61/61   | 27/27     | 0         |       0 |

- Baseline registered ~45–55 % of detents, matching the real-redraw loss measured at `dfb3f45`, so the busy-wait is a fair stand-in.
- A ran the unchanged async `Encoder` on the interrupt executor; ~4.1 edges per detent, no skips.
- B saw ~1.3 ISRs per level change (bounce, coincident A/B edges); none reached the decoder as a skip.
- Choice: B — it gives the esp-hal tier the IDF tier's sync shape (ISR keeps `position()` current), so consumers switch tiers by changing the import and constructor.
- B shape: new `rotary::InterruptEncoder`; the app installs its GPIO `#[handler]` and calls `rotary::on_gpio_interrupt()`; async `Encoder` unchanged.

## Session Log
- 2026-10-26 — Feature doc created via /feature dialog from a downstream request (knob, CrowPanel load test).
- 2026-10-27 — Phase 1 done: `QuadratureDecoder::skipped_count()` (+ `QuadratureInput`, esp-hal `Encoder` pass-throughs), budget rustdoc in the esp-hal rotary module. IDF tier unchanged. Phase 2 waits on the CrowPanel.
- 2026-10-27 — Spikes A and B both lossless under a 90 ms blocking redraw (baseline lost ~half); B chosen for tier parity, app-installed GPIO handler. Added `just capture` for non-interactive serial logging.
- 2026-10-27 — `rotary::InterruptEncoder` + `on_gpio_interrupt()` implemented after specialist review (register-level hook, no lock inversion); `hal_s3_rotary_isr` lossless on the CrowPanel (2×62/62, 62/62, 26/26, delta 0, skipped 0), click/double/long press OK. Added `just clippy-hal` / `doc-hal`.
- 2026-10-28 — Lifecycle hardening after external review (new/drop race, seed-to-arm gap, doc qualifications). Downstream knob validated `b65dad5` with the real ~90 ms GC9A01 redraw: 60/60 normal, 60/60 fast (24 dps, 27 redraws), reversals net 0, skipped 0. Feature complete.
