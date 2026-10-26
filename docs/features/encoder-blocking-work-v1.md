---
gate: hardware
gate-note: CrowPanel 1.28" S3 load test with a ~90 ms blocking redraw to pick and validate the phase-2 approach
desk-work: available
---

# Feature: esp-hal Encoder Under Blocking Work v1

The async esp-hal `rotary::Encoder` decodes in task context, so blocking work between two `wait_for_step` calls drops detents.
Measured on the CrowPanel 1.28" S3 at `dfb3f45` (30 detents/rev): lossless up to ~13 ms of work per step, ~50–63 % of detents lost under a ~90 ms blocking SPI redraw.
A rotation-only task on the same cooperative executor is still stalled, so `split()` alone does not fix it.

**Phase 1 — make starvation visible:** state the measured budget in rustdoc, and count 2-bit jumps in `tamer::rotary` so consumers can detect lost steps.
**Phase 2 — make decoding survive the redraw:** spike both candidates below, pick one on hardware, record it as an ADR-006 amendment.

## Decisions
|                                                 Decision | Reason                                         | Rejected Alternative                        |
|---------------------------------------------------------:|:-----------------------------------------------|:--------------------------------------------|
|                                      One doc, two phases | One consumer report, one acceptance criterion  | Separate `tamer` counter feature            |
|               Lost-step counter lives in `tamer::rotary` | Pure Gray-code logic, host-testable            | Counting in the esp-hal tier                |
|        Spike both isolation candidates before committing | Hardware decides; both have real costs         | Adopting the request's `split()` plan as-is |
| Existing `Encoder` API stays usable for simple consumers | Request acceptance; ADR-006 shape is validated | Replacing `wait_for_step`                   |

## Constraints
- Phase-2 candidate A: `Encoder::split()` plus the rotation half on an esp-rtos interrupt-priority executor, shown in `hal_s3_rotary_isolated`.
- Phase-2 candidate B: decode inside a GPIO interrupt handler, mirroring the IDF tier, at the cost of statics and `critical_section`.
- The counter must not change `QuadratureDecoder::update` results; a 2-bit jump still yields `None`.
- The counter must stay compatible with the arithmetic decode planned in [IRAM-safe ISR v1](iram-safe-isr-v1.md).
- The shared encoder trait (ADR-006) stays the synchronous core; phase 2 must not force an async trait.
- Tests cover CW and CCW symmetrically.

## Open Questions
- [ ] Candidate A or B — which one gets 30/30 detents under the ~90 ms redraw?
- [ ] Counter API shape: monotonic `skipped_count()` vs. take-and-clear?
- [ ] Does a 2-bit jump from contact bounce inflate the counter enough to make it noisy at speed?
- [ ] Land phase 2 before, with, or after the shared encoder trait extraction?

## State
- [x] Design approved
- [ ] Phase 1: rustdoc states the measured ~10 ms per-step budget
- [ ] Phase 1: `tamer::rotary` lost-step counter with host tests
- [ ] Phase 2: spike A and B on the CrowPanel load test
- [ ] Phase 2: ADR-006 amendment records the choice
- [ ] Phase 2: implementation + example; 30/30 detents with a ~90 ms redraw
- [ ] CHANGELOG, README

## Session Log
- 2026-10-26 — Feature doc created via /feature dialog from a downstream request (knob, CrowPanel load test).
