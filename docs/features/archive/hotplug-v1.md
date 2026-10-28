---
gate: none
desk-work: available
---

# Feature: Hot-Plug Attach/Detach v1

A pure state machine that decides when a removable device has attached or detached, from the outcome of each probe or transfer.
Reference use: an SSD1306 OLED on a magnetic pogo connector, unplugged while the node runs.
The machine never touches a bus; the caller feeds `ok: bool` (e.g. the ADR-004 zero-length-write probe, NACK → `false`).

```text
pub struct Hotplug
Hotplug::new(attach_debounce: u64, detach_errors: u32) -> Hotplug
Hotplug::observe(&mut self, ok: bool, now: u64) -> Option<Transition>
Hotplug::reset(&mut self)
pub enum Transition { Attached, Detached }
```

## Decisions
|                                 Decision | Reason                                      | Rejected Alternative             |
|-----------------------------------------:|:--------------------------------------------|:---------------------------------|
|                Build now on one consumer | Maintainer waiver: small, well-shaped, pure | Park until a second consumer     |
|   Attach via `presence::DigitalPresence` | Reuses the ADR-002 debounce, no duplicate   | A private debounce window        |
|      Detach by consecutive failure count | Transfer errors are discrete, not sampled   | Time-debounced detach            |
|                    Unit-free `u64` ticks | Matches every other `tamer` timer           | Hard-coded milliseconds          |
|  `detach_errors == 0` panics, documented | `AnalogRange::new` precedent                | `NonZeroU32` param; silent clamp |
| Module `tamer::hotplug`, no feature gate | Names the mechanism, not the OLED           | `display`-slot placement         |

## Constraints
- `no_std`, alloc-free, no new dependency.
- Detached: `ok` must hold for `attach_debounce` without a gap; one `false` restarts the window.
- Attached: `detach_errors` consecutive `false` detach; any `true` resets the count.
- `observe` returns a transition only on a change.
- `reset()` drops to detached and emits nothing.
- No `Noop*` mock: the machine has no hardware-interaction trait.
- Chip-tier probe and bus-recovery glue (SCL clock-out, driver rebuild) are out of scope.

## Open Questions
- [x] Should `Hotplug` expose `is_attached()` for callers that skip the transition? — yes

## State
- [x] Design approved
- [x] Core implementation
- [x] Tests: continuous-window attach; bounce restarts window; one failure tolerated
- [x] Tests: `detach_errors` failures detach; `detach_errors = 1` detaches on first; `reset()` silent
- [x] Rustdoc with doctest, CHANGELOG, `lib.rs` re-export

## Session Log
- 2026-10-26 — Feature doc created via /feature dialog; two-consumer rule waived by the maintainer.
- 2026-10-26 — Implemented `tamer::hotplug` with unit tests, doctests, CHANGELOG entry, and `lib.rs` re-exports.
- 2026-10-26 — Review follow-up: documented probe cadence and observation policy; added reattach and sparse-probe tests.
