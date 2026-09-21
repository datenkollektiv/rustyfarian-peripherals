---
gate: none
desk-work: available
---

# Feature: Presence Session (`tamer::presence::PresenceSession`) v1

A warm-up-gated session state machine over a debounced `Presence` level stream: counts sessions, measures dwell, and hides the boot pulses of latched sensors such as the HC-SR501 PIR module.

## Decisions
|                                                       Decision | Reason                                                                           | Rejected Alternative                                             |
|---------------------------------------------------------------:|:---------------------------------------------------------------------------------|:-----------------------------------------------------------------|
|                     `PresenceSession` inside `tamer::presence` | Named for the mechanism; lives next to the `Presence` it consumes                | `tamer::motion::MotionSession` (use-case name); new `session.rs` |
|     Positional `new(initial_level, warmup, trigger_mode, now)` | Family convention: no config struct, initial state first, new arg last           | `MotionConfig` struct                                            |
| `update(level: Presence, now: u64)` takes the level every poll | Arming on "first `Absent` at or after warm-up end" needs the level, not edges    | Consuming `Option<Presence>` transitions                         |
| `TriggerMode` kept as caller-declared metadata, zero behaviour | Mirrors the HC-SR501 H/L jumper; readable back for display; one-byte `Copy`      | Drop it and document the jumper in module docs                   |
|                                   No `#[must_use]` on `update` | Every sibling `update` omits it; poll loops discard `None` legitimately          | `#[must_use]` on `update` as proposed                            |
|                    No trait, no `Noop*` mock, no `hal` adapter | The level feed is the seam, as in `hall`, `mpu6050`, `touch`                     | Session trait plus mock                                          |
|       Clock regression is not detected; elapsed math saturates | One policy, same as `Debouncer` / `TouchTracker`; no hidden "ignore sample" path | Ignore samples whose `now` regresses                             |

## API surface
- `SessionPhase::{WarmingUp, Settling, Armed}`, `TriggerMode::{Repeatable, NonRepeatable}` (no `Default`), `SessionEvent::{WarmupPulse, Started { session: u32 }, Ended { session: u32, dwell: u64 }}`; all `Debug, Clone, Copy, PartialEq, Eq`.
- `const` queries `phase`, `level`, `is_active`, `event_count`, `spurious_count`, `last_onset`, `last_dwell`, `max_dwell`, `trigger_mode`; dwell queries are `Option<u64>` because a dwell of `0` is legitimate.
- `warmup`, `now`, and `dwell` share one caller-selected tick unit; the crate never assumes milliseconds.

## Behavioral contract
|     Phase | Sample                          | Effect                                         | Returns                     |
|----------:|:--------------------------------|:-----------------------------------------------|:----------------------------|
| WarmingUp | `now < warmup_end`, rising edge | `spurious` saturating `+1`                     | `Some(WarmupPulse)`         |
| WarmingUp | `now < warmup_end`, other       | none                                           | `None`                      |
| WarmingUp | `now >= warmup_end`             | become `Settling`, re-evaluate this sample     | as `Settling`               |
|  Settling | level `Absent`                  | become `Armed`                                 | `None`                      |
|  Settling | level `Present`                 | stay; not counted as spurious                  | `None`                      |
|     Armed | rising edge                     | `events` saturating `+1`, `last_onset = now`   | `Some(Started { session })` |
|     Armed | falling edge                    | `dwell = now - onset`; update last / max dwell | `Some(Ended { .. })`        |
|     Armed | same level                      | none                                           | `None`                      |

- `warmup_end = now_at_construction.saturating_add(warmup)` saturates at `u64::MAX`; the warm-up then ends only when `now` reaches `u64::MAX`, and nothing panics.
- Only the sample that first crosses `warmup_end` is re-evaluated as `Settling`: a rising edge there is silent and not spurious, while one tick earlier it is a counted `WarmupPulse`.
- Once `Armed`, every rising edge is a `Started`; boundary suppression never applies to an armed session.
- Construction never arms; arming requires an `Absent` sample observed through `update`, so with `warmup == 0` a first `Present` sample is suppressed as `Settling` (intentional).
- `initial_level` seeds the stored level, so a sensor already `Present` at boot produces no rising edge.
- Clock regression is not detected: samples are processed normally, `now.saturating_sub(onset)` clamps dwell to `0`, and no transition depends on `now` decreasing, so phases never revert.
- Dwell is the observed signal duration: ticks between the accepted debounced `Present` sample and the accepted debounced `Absent` sample, nothing more.
- `TriggerMode` changes only the sensor-specific reading of dwell: HC-SR501 H jumper (`Repeatable`) retriggers while motion continues, L jumper (`NonRepeatable`) yields roughly the delay-pot time.
- `Armed` covers both idle and active sessions; `level()` tells them apart, or `is_active()` combines both checks into one call.
- `last_onset` starts `None`, is set on `Started`, and is retained after `Ended` until the next `Started`; `last_dwell` and `max_dwell` start `None`, change only on `Ended`, and keep the previous completed value during an active session.
- `Started.session` is 1-based and equals `event_count()` after the call; `event_count` and `spurious_count` saturate at `u32::MAX`, after which successive sessions reuse that value, so `session` is a count, not a unique session id.

## Polling example
```rust
let mut pir = DigitalPresenceInput::try_from_pin(pin, Polarity::ActiveHigh, 0)?;
let mut session = PresenceSession::new(pir.stable_state(), 60_000, TriggerMode::Repeatable, now_ms());
loop {
    let now = now_ms();
    pir.update(now)?;
    if let Some(event) = session.update(pir.stable_state(), now) {
        handle(event);
    }
}
```
Update the detector first, then feed `stable_state()` on every successful poll, including polls with no transition.

## Constraints
- `no_std`, alloc-free, zero new dependencies, no feature flag; only the composition test is `hal`-gated.
- No `unwrap` / `expect` / `panic!` in library paths; all tick arithmetic via `saturating_*` as in `Debouncer` and `TouchTracker`.
- Purely additive semver; `SessionEvent` stays exhaustive like `ButtonEvent`, so v2 growth is an accepted 0.x bump.
- `presence.rs` roughly doubles to the size of `touch.rs`; accepted for co-location with `Presence`.

## Required tests
- Warm-up: rising edge counts one `WarmupPulse`, falling edge is `None`; `Absent` at exactly `warmup_end` arms directly.
- Zero warm-up: initial `Absent` then a first `Present` sample stays `Settling` with no event; a first `Absent` sample arms.
- Boundary: a rising edge at `warmup_end` and one tick after are silent and not spurious; `Absent` at `warmup_end` then `Present` one tick later is a `Started`; `Present` across `warmup_end` stays `Settling`, and the next `Absent` arms without an `Ended`.
- Armed: `Absent → Present → Absent` yields `Started` then `Ended` with dwell; `last_dwell` and `max_dwell` track two sessions of different length; `last_dwell` holds the previous value during an active session.
- Robustness: a deadline saturated at `u64::MAX` stays `WarmingUp` for every `now < u64::MAX`, and at `u64::MAX` an `Absent` arms while a `Present` settles without a pulse; tick regression across `warmup_end` and inside an open session gives dwell `0` and no phase reversion.
- Idempotence and saturation: same `(level, now)` twice is a no-op; `events` and `spurious` hold at `u32::MAX` and the next `Started` reuses `u32::MAX`; `initial_level = Present` produces no rising edge; both `TriggerMode`s yield identical streams.
- One `hal`-gated test composing `DigitalPresenceInput<MockInputPin>` with a zero window into `PresenceSession`.

## Deferred (v2+)
- Hardware example twin (`hal_c3_pir` / `idf_c3_pir`) once a bench unit is free; minimum-dwell or cooldown filtering; a generic `tamer::session` module if a non-`Presence` consumer appears.

## State
- [x] Design approved (2026-09-21: naming and `TriggerMode` decided by maintainer; rust-engineer review folded in)
- [x] Core implementation (2026-09-21: `PresenceSession` in `presence.rs`, root and `prelude` re-exports, `lib.rs` Status bullet)
- [x] Host tests passing (2026-09-21: `just verify` 254 + 32 doctests, `just test-hal` 278 + 37 doctests, both exit 0; 21 new `presence::tests`)
- [x] Documentation updated (2026-09-21: module and `update` docs, CHANGELOG `[Unreleased]` / Added; `just doc-all-features` 0 warnings)

## Session Log
- 2026-09-21 — Feature doc created via `/feature` from a downstream request; rust-engineer review fixed constructor order, boundary-edge semantics, and `#[must_use]` consistency.
- 2026-09-21 — Maintainer review: one clock-regression policy, honest deadline saturation, boundary suppression limited to unarmed sessions, dwell defined as observed signal duration, query and counter contracts completed, polling example, zero-warm-up startup made explicit, `test-hal` listed separately.
- 2026-09-21 — Implemented via rust-engineer, code-reviewer gate PASS-WITH-NITS (three intra-doc links fixed); all gates green; uncommitted in the working tree.
- 2026-09-21 — Second maintainer review: `hal`-only doc links made default-feature safe, two boundary tests added (deadline at `u64::MAX`, arm-then-rise at the deadline), `Repeatable` dwell wording fixed; all gates green.
- 2026-09-26 — External review acted on: `SessionEvent` ordinal field renamed `event` → `session` (maintainer's pick; `event_count()` / `spurious_count()` deliberately unchanged), `is_active()` added with the `is_active_tracks_open_session` test, CHANGELOG now states the `warmup == 0` suppression. Rejected: renaming `TriggerMode` (it is the HC-SR501 datasheet's own term for the H/L jumper, so it names the mechanism), `is_idle()` (would widen "idle" beyond the `Armed && Absent` meaning the docs already fix), and moving `## State` / `## Session Log` out (repo-wide `template.md` convention, all 10 feature docs). All gates green.
- 2026-09-26 — Second review round: found that neither doctest actually demonstrated the `warmup == 0` suppression (both started from `Absent` and armed immediately), so the surprising path lived only in a unit test. Added a dedicated default-feature doctest under "Zero warm-up still requires an `Absent` sample" and promoted the suppression to its own sentence in the CHANGELOG. `TriggerMode` rejection re-affirmed. All gates green (33 doctests, 38 with `hal`).
