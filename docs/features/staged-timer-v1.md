# Feature: Staged timer v1

A pure schedule primitive, `tamer::staged`, for a downstream consumer that runs a slowing-down step animation.
It says when a step is due; what a step does stays downstream.

## Decisions
| Decision | Reason | Rejected Alternative |
|----------|--------|----------------------|
| Names `staged`, `StagedTimer`, `Stage`, `StagedEvent { Step, Finished }` | Describe the mechanism, not the first consumer; `Stage` reads as a run of steps and avoids the generic `Phase` | `Phase` / `Done` — too generic |
| API mirrors `ToneSequencer` (borrowed table, `u64` ticks, one event per update) | One idiom across schedule primitives | A new clock model |
| First step fires at the start tick | A step is the unit of work; no implicit step 0 | Delay by one interval (as note 0 plays implicitly in `tone`) |
| Empty or all-zero table emits `Finished` once on first update | The caller learns completion uniformly through an event | Silent already-finished, as `ToneSequencer` |
| Absolute deadlines with `saturating_add` | No drift, no overflow; a late poll never stretches the schedule | Rebase to poll time |
| `StagedMode { Once, Loop }` passed positionally to `new`; `Loop` wraps to the first non-empty stage and never emits `Finished` | A downstream dice prop runs endlessly (tumble, slow, settle, hold, repeat); `ToneSequencer` parity | Reusing `tone::SequenceMode` — couples modules |
| Empty or all-zero table finishes once in both modes | No infinite zero-time loop | Looping an empty table |
| `stage() -> Option<usize>`: table index of the stage of the latest `Step`, counting zero-count stages; `None` until the first step; kept across `stop`/`Finished`, reset by `start` | The hold is just a final one-step stage; the caller needs to tell it apart without a new event variant | A `StageStep(usize)` event variant, or a separate `Hold` event |
| Dice-specific presets, faces, RNG stay downstream | Not a `tamer` concern | A `CLASSIC_ROLL` preset |

## Constraints
- `no_std`, alloc-free, no dependencies, panic-free.
- Positional `const fn new`; `StagedEvent` is exhaustive; `Stage` has public fields.

## Open Questions
- None.

## State
- [x] Design approved
- [x] Core implementation
- [x] Tests passing
- [x] Documentation updated
