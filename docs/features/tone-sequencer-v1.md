# Feature: Tone/Duration Sequencer v1

A pure, sans-IO tone/duration sequencer — a "melody player" state machine —
for `tamer`. Steps through a caller-owned, borrowed `&[Note]` table (frequency
+ duration + amplitude per step), once (`OneShot`) or looped (`Loop`), and
reports the tone a downstream buzzer/speaker adapter should be producing via
a separate `output()` query. This lands `tamer`'s **first output/actuator
primitive** — the module imports no PWM, DAC, I2C, or chip crate; the
hardware write is entirely downstream.

## Background

- **Why `tamer`.** Already named in `VISION.md` / `docs/ROADMAP.md` as "tone/duration sequencing" under "input *and* output" peripherals; `range_map` sits on the input→output seam; this is the first output-shaped module. Extends the charter, not an ad hoc surprise.
- **Why mechanism name (`tone`).** Matches the crate convention (`debounce`, `rotary`, `button`, etc.); no chip coupling or datasheet justifies a device name (unlike `mpu6050`).
- **Why borrowed `&'notes [Note]` slice.** Zero-alloc, `Copy`-cheap; a melody is naturally a `const` table (matches `mpu6050::INIT_SEQUENCE`'s precedent). Owned `const N`-buffer reversal is breaking either way.

See [`review-queue/tone-sequencer-donation-v1.md`](../../review-queue/tone-sequencer-donation-v1.md) for the full rationale.

## Decisions

|                                                                     Decision | Reason                                                                                                                         |
|-----------------------------------------------------------------------------:|:-------------------------------------------------------------------------------------------------------------------------------|
|                      Land as `tamer::tone` — first output/actuator primitive | `VISION.md`/`ROADMAP.md` already name this; `range_map` established the input→output seam                                      |
|                         Name `tone` (mechanism), not `buzzer` or device name | Matches crate convention; no chip coupling                                                                                     |
|                    Borrow `&'notes [Note]` instead of owned `const N` buffer | Zero-alloc, matches `mpu6050::INIT_SEQUENCE` precedent                                                                         |
|           Tick contract mirrors `debounce`/`button`: `u64`, `saturating_sub` | Consistency with established sibling primitives                                                                                |
|      `output()` is separate side-effect-free query, not folded into `update` | Lets downstream adapter re-read without `Option` branching (mirrors `presence::DigitalPresence`)                               |
|                      Rest is `frequency_hz == 0`, not a separate `Step` enum | Single flat `Copy` type; `0 Hz` is musically a rest anyway                                                                     |
|              Boundary is inclusive: advance when `elapsed >= duration_ticks` | Matches `Debouncer`'s exact-boundary contract                                                                                  |
| Advance accumulates schedule (`note_since += duration`), not rebase to `now` | **Deliberate divergence** from edge detectors: sequencer must not drift under jittery polling; rebasing would stretch playback |
|                           `output()` coerces `frequency_hz == 0` to `SILENT` | `0 Hz` means rest everywhere; makes coerced output reliable                                                                    |
|          Zero-duration notes advance one per `update`, never loop internally | Matches `ButtonDecoder`'s "at most one event per call"; avoids stalling on pathological data                                   |
|                                               Empty slice is always finished | Panic-free by construction; matches `SlidingAverage::<0>` spirit                                                               |

## Behavioral contract

The mapping the implementation and its tests must satisfy.

- **Advance is boundary-inclusive.** A note holds while `now.saturating_sub(note_since) < duration_ticks`; it advances the instant `elapsed >= duration_ticks`. Tested at the exact boundary and one tick on either side.
- **Boundaries accumulate the schedule; polling jitter does not drift.** On advance, the baseline moves by the expiring note's duration (`note_since = note_since.saturating_add(duration_ticks)`), **not** rebased to `now`. A late or coarse `update(now)` does not stretch the melody or bake poll lateness into later notes — the schedule stays absolute. **Deliberate divergence** from edge detectors: tested with coarse and jittery polling.
- **At most one `SequenceEvent` per `update` call.** A run of zero-duration notes advances one call at a time; `update` never loops internally past more than one note boundary. A behind caller catches up one note per call; work per call is bounded.
- **A `0 Hz` note is always fully silent.** `output()` reports `ToneOutput::SILENT` for any current note with `frequency_hz == 0`, whatever its amplitude. `frequency_hz == 0` means "rest" everywhere.
- **`Finished` fires exactly once.** For `OneShot`, reaching the end transitions to inactive/finished and returns `Some(SequenceEvent::Finished)` once only; every subsequent `update` returns `None`, every `output()` returns silence, until `start()` is called again.
- **`Loop` never finishes.** Reaching the end wraps to note 0, emits `Some(SequenceEvent::NoteChanged(0))`, and `is_finished()` stays `false` indefinitely.
- **Rests are silent but consume time.** `Note::rest(duration_ticks)` (equivalently `frequency_hz == 0`) makes `output()` report `ToneOutput { frequency_hz: 0, amplitude: 0 }` for the rest's full duration, then advances normally.
- **Non-monotonic `now` never panics or advances spuriously.** All elapsed arithmetic uses `now.saturating_sub(note_since)`; out-of-order timestamps clamp elapsed to zero, delaying advance rather than producing one.
- **Empty slice is total.** `ToneSequencer::new(&[], _)` is immediately `is_finished() == true`, `output()` is silent, and `start()`/`update()` are safe no-ops with no index OOB — the empty case is checked once at construction, not defensively re-derived every call.
- **`start()` always resets to note 0 with fresh baseline.** Sets `index = 0` and `note_since = now` regardless of current state; timing is always measured from the new start point.

## Constraints

- Pure `no_std`, alloc-free, host-testable; **no PWM / DAC / I2C / HAL / chip crate coupling**. MSRV 1.88.
- Zero-allocation: only a slice reference and scalar fields (`bool`, `usize`, `u64`); no heap.
- Tick type and non-monotonic handling match `debounce`/`button`: caller-owned `u64`, `saturating_sub`. **One deliberate divergence:** accumulate schedule on advance, not rebase to `now`.
- Value types: `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]` (all fields are integer/enum data); `const fn` constructors; `#[must_use]` on `Option`/state-returning methods; no `Result` in public API.
- No panicking paths anywhere in the module, including empty-slice case.
- No hardware trait → **no `Noop*` mock required** (matches `range_map` / `smoothing` precedent).
- Never imports `rustyfarian-ws2812` — hard `VISION.md` boundary.

## Module & API surface (v1)

- `tamer::tone` — `Note`, `SequenceMode`, `ToneOutput`, `SequenceEvent`, `ToneSequencer<'notes>`.
- **Prelude**: `Note`, `SequenceMode`, `ToneSequencer` (hero types, mirrors `mpu6050`/`smoothing` precedent). `ToneOutput` and `SequenceEvent` excluded — return/query value types referenced by fully-qualified path.
- No `hal` feature surface: this module produces values only; hardware write is entirely downstream.

## Required tests (host)

- Boundary behavior: advance at the exact boundary, one tick before/after.
- Rests: `output().frequency_hz == 0` while active, then normal advance.
- Zero-duration notes: advance one per `update`, no internal loop.
- Schedule preservation: coarse `update` advances exactly once and leaves baseline on schedule (no drift).
- Jitter tolerance: slightly-late poll does not shift later boundaries.
- Zero-Hz silence: `Note::new(0, d, 255)` yields `output() == ToneOutput::SILENT`.
- `OneShot` completion: `Finished` fires once; subsequent `update`/`output` correct.
- `Loop` wraparound: wraps to note 0, never finishes (tested two cycles).
- Empty slice: `is_finished() == true` immediately, `output()` silent, no panic/OOB on `start`/`update`.
- Non-monotonic `now`: earlier timestamp saturates elapsed to zero.
- Near-`u64::MAX`: transitions correctly at wrap-adjacent boundary; `OneShot` fires `Finished` at `u64::MAX`.
- `start()` restart: resets to note 0 and re-baselines from new `now`; calling after `Finished` clears flag and resumes from note 0.
- `stop()`: silences output and suspends `update` until next `start`.

## Deferred (explicitly decided — not open)

- **Hardware adapter (DAC/PWM)** → downstream. Matches how `mpu6050`'s I2C read stays with caller. Future tier adapter possible once a second consumer needs the glue.
- **Volume/amplitude curve** → uninterpreted. Downstream adapter owns the mapping (linear PWM duty, DAC scale, etc.).
- **Tempo scaling / transposition helpers** → demand-driven (speculative until a real consumer needs them).

## Resolved

- **`'notes` lifetime API lock-in** → LOCKED as final for v1. Shipped and hardware-validated on ESP32-C3-MINI-1; owned-buffer reversal is breaking either direction.
- **Prelude cut** → confirmed as-is. `Note`/`SequenceMode`/`ToneSequencer` in prelude; `ToneOutput`/`SequenceEvent` out (hero types / return value types).
- **CONTRIBUTING.md "input layer" framing** → fixed (see Task 1a; changed to "input and output primitives").
- **Cargo.toml/lib.rs input-only descriptions** → fixed (see Task 1b/1c; broadened to "input and output primitives"; tone/duration added to parentheticals).
- **Amplitude `u8` uninterpreted** → confirmed. Curve stays downstream (see Deferred).

## State

- [x] Design approved
- [x] Core implementation + prelude entries
- [x] Host tests passing: 166 unit + 20 doctests (default); 27 unit (all-features)
- [x] First downstream consumer: ESP32-C3 buzzer examples (`hal_c3_buzzer`, `idf_c3_buzzer`); per-note frequency retune on both tiers
- [x] CHANGELOG + ROADMAP updated
- [x] Timing-drift fix (accumulate schedule, not rebase) + silence coercion on `0 Hz`; coverage gap tests added
- [x] Real-hardware validation: both examples flashed and run on ESP32-C3-MINI-1
- [x] Documentation wrap-up: all open questions resolved (borrowed-slice API locked as v1-final); input-only framing corrected across CONTRIBUTING.md / Cargo.toml / lib.rs; feature doc condensed. **Feature complete.**

## Session Log

- 2026-07-13 — Feature doc + donation request created; core implementation with required-test coverage (boundary, rests, zero-duration, `OneShot`/`Loop` completion, empty slice, non-monotonic clock, near-`u64::MAX`, `start()`/`stop()` semantics). Flagged `'notes` lifetime as hardest-to-reverse decision. `just verify` green (166 + 20 doctests).
- 2026-07-14 — First consumer landed: ESP32-C3 buzzer examples on both tiers, per-note frequency retune via LEDC PWM. APB-clock correction in docs. `just verify` green (167 unit + 20 doctests).
- 2026-07-14 — External PR feedback fix: corrected timing-drift bug (accumulate schedule, not rebase); coerced `0 Hz` to `SILENT`. Added `loop_wrap_schedule_survives_coarse_polling` + `accumulate_survives_zero_duration_notes_under_coarse_polling` + 0-Hz silence test. `just verify` green (172 unit + 20 doctests).
- 2026-07-15 — Real-hardware validation: both `hal_c3_buzzer` and `idf_c3_buzzer` run on ESP32-C3-MINI-1; per-note retune verified on silicon.
- 2026-07-15 — Doc wrap-up: all open questions resolved (borrowed-slice API locked as v1-final); input-only framing corrected across CONTRIBUTING.md / Cargo.toml / lib.rs; feature doc condensed from 355 to 140 lines. Feature complete and locked for release.
