---
gate: none
desk-work: available
---

# Feature: Presence Session Resume v1

Snapshot and restore for `tamer::presence::PresenceSession`, so a battery device can resume a session after deep sleep instead of re-warming.

## Decisions
|                                                       Decision | Reason                                                              | Rejected Alternative                       |
|---------------------------------------------------------------:|:--------------------------------------------------------------------|:-------------------------------------------|
|                   `SessionState` plain-data struct, pub fields | Consumer serialises it into its own retained record                 | Storing `PresenceSession` (private fields) |
|                                Exhaustive struct, no `Default` | Family convention; a new field later is an accepted 0.x bump        | `#[non_exhaustive]` + long const `new`     |
|           Relative ticks: `warmup_remaining`, `last_onset_age` | Ticks restart at zero after deep sleep; absolute values go stale    | Absolute `warmup_end` / `last_onset`       |
|                    `SessionState::aged_by(elapsed)` in `tamer` | Pure saturating math; one implementation for both tiers             | Consumer-side helper per firmware stack    |
| `restore` normalises Armed + Present + no onset to onset `now` | Otherwise the next falling edge is swallowed; `update` untouched    | `Ended { dwell: 0 }` branch in `update`    |
|                No `resume_on_wake`; caller feeds one `Present` | A GPIO wake is a `Present` sample; the normal rising edge counts it | Wake-aware API inside `tamer`              |
|                                    `warmup_end()` const getter | Consumers stop duplicating the deadline                             | Keep it private                            |

## API surface
- `SessionState { trigger_mode, phase, level, warmup_remaining, events, spurious, last_onset_age, last_dwell, max_dwell }`, deriving `Debug, Clone, Copy, PartialEq, Eq`.
- `PresenceSession::snapshot(&self, now) -> SessionState`, `PresenceSession::restore(state, now) -> Self`, `PresenceSession::warmup_end(&self) -> u64`, `SessionState::aged_by(self, elapsed) -> Self`.
- `PresenceSession::is_idle(&self) -> bool`: Armed and `Absent`, the snapshot rule, for sleep-entry code.
- All five are `#[must_use] pub const fn`; `Option` handling uses `match`, max uses `if`, since `Option::map` and `Ord::max` are not const on 1.88.

## Behavioral contract
- `snapshot`: `warmup_remaining = warmup_end.saturating_sub(now)`, `last_onset_age = now.saturating_sub(onset)` plus the private onset deficit (saturating); everything else is copied verbatim.
- `restore`: `warmup_end = now.saturating_add(warmup_remaining)`, `last_onset = now.saturating_sub(age)`, clamped to `0` when `now < age`; the excess `age.saturating_sub(now)` is kept in a private `onset_deficit`, so `snapshot(now)` reports the exact age `age + (now - restore_tick)`, saturating.
- `last_onset()` returns the clamped tick; a rising edge in `update` resets the deficit to `0`.
- The `Ended` dwell of a window open across a restore is measured from the clamped onset and stays approximate.
- If the restored `level` is `Present`, the follow-up `Present` sample produces no `Started`.
- `restore` never panics and normalises exactly two cases: Armed + Present with no onset gets onset `now`, and `max_dwell` is raised to at least `last_dwell`.
- Every other field combination is carried verbatim; outside `WarmingUp` the rebased deadline has no behavioral effect.
- `aged_by`: `warmup_remaining` saturating `-elapsed`, `last_onset_age` saturating `+elapsed`; the phase never changes, and the next `update` performs any transition.
- Snapshot only while idle (Armed + `Absent`); an open window does not carry its true onset across a restore, so a later dwell can come out short.
- A wake encounter's dwell is measured from the restore, so it is short by the boot latency.
- `new` and `update` behaviour is unchanged.

## Constraints
- `no_std`, alloc-free, no new dependencies, no feature flag, no sleep or clock awareness in `tamer`.
- Purely additive semver; all existing `presence::tests` pass unchanged.
- Re-export `SessionState` from the crate root and `prelude`; add a `lib.rs` Status bullet and a CHANGELOG `[Unreleased]` / Added entry.

## Required tests
- Round trip at the same `now` preserves every getter, and `warmup_end()` while the deadline is pending (a passed one rebases to `now`); `restore(s, n).snapshot(n) == s` for a normalised `s`.
- Idle Armed state restored at `0`: `Present` gives `Started { session: events + 1 }`, then `Absent` at `3` gives `Ended { dwell: 3 }`.
- Warm-up rebasing: remaining `10` restored at `5` arms on `Absent` at `15` and not before; restore into `WarmingUp` and into `Settling` behaves like a fresh session at that phase.
- Onset age: onset `40` snapshotted at `100` gives age `60`, restored at `100` gives `Some(40)`, and restored at `0` gives `Some(0)`.
- Normalisation: Armed + Present + no age, then `Absent`, gives `Ended`; `max_dwell < last_dwell` is clamped.
- `aged_by(0)` is identity, `aged_by` saturates both fields, and remaining reaching `0` arms on the next `Absent` sample.
- Dwell stats survive a restore; `NonRepeatable` survives a round trip; fifty snapshot/restore/`Started`/`Ended` cycles keep `event_count()` gap-free; `u64::MAX` inputs never panic.
- One doctest showing the idle-only sleep and resume flow.

## Deferred (v2+)
- `SessionState::to_le_bytes` / `from_le_bytes` fixed byte codec: the consumer has a local codec and will promote it after field proof.
- Hardware example pair (`hal_c3_pir` / `idf_c3_pir`), minimum-dwell or cooldown filtering, and a generic `tamer::session` module.

## State
- [x] Design approved (2026-10-12: invariant, ageing and exhaustiveness decided by maintainer; rust-engineer review folded in)
- [x] Core implementation (2026-10-12: `SessionState`, `snapshot` / `restore` / `warmup_end` / `aged_by` in `presence.rs`, root and `prelude` re-exports)
- [x] Tests passing (2026-10-12: 18 new `presence::tests` plus a `SessionState` doctest; gates below)
- [x] Documentation updated (2026-10-12: rustdoc, `lib.rs` Status bullet, CHANGELOG `[Unreleased]` / Added)
- [x] Review follow-up (2026-10-12: `is_idle`, restore-`Present` note and tests, exact onset age across restore, doc nits, 7 new `presence::tests`; byte codec deferred)

## Session Log
- 2026-10-12 — Feature doc created via `/feature` from a downstream resume request; rust-engineer review added `aged_by` and the `max_dwell` clamp.
- 2026-10-12 — Implemented via rust-engineer, code-reviewer gate PASS-WITH-NITS (round-trip test bullet and `warmup_remaining` docs corrected); uncommitted.
- 2026-10-12 — Review follow-up: added `is_idle`, documented and tested the restored-`Present` case, fixed `last_onset_age` collapse across restore with a private `onset_deficit`; the byte codec is deferred.
