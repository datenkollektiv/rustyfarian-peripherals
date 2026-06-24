# ADR-002: Digital presence detection

## Status
Accepted

## Context
Some GPIO inputs represent physical presence rather than button gestures.
Examples include reed switches, beam breaks, PIR modules, digital Hall switches, and capacitive touch modules.

`rustbox-peripherals-pure` added this boundary as `input::DigitalPresence`.
The same need exists here now that `tamer` already owns the shared `Debouncer` contract.

Without a named primitive, callers compose polarity mapping and debouncing by hand.
That ceremony is small, but it is easy to repeat inconsistently and tends to pull reed switches or beam breaks into button-shaped APIs.

## Decision
Add `tamer::presence`.
It contains `Presence`, `Polarity`, and `DigitalPresence`.

`DigitalPresence` accepts raw boolean readings and caller-supplied `u64` ticks.
It maps raw levels through `Polarity`, debounces the semantic present boolean with `Debouncer`, and returns debounced `Presence` transitions.

The `hal` feature also exposes `DigitalPresenceInput<P>`.
It is a thin `embedded-hal` `InputPin` adapter that follows the existing `tamer` pattern of `new`, `try_from_pin`, `update`, `stable_state`, and `pin_mut`.

This is a clean reimplementation from the donor repository rather than a verbatim copy.
It intentionally follows this repo's already-documented zero-debounce behavior: a `0` debounce window confirms the first changed sample immediately.

## Consequences
- Reed switches, beam breaks, PIR modules, and digital Hall switches get a semantic present / absent path without gesture vocabulary.
- Button click, double-click, and long-press behavior remains in `tamer::button`.
- Hardware tiers can validate raw GPIO wiring by printing debounced `Presence` transitions.
- The public API grows by one small module and one optional `hal` adapter.

## Alternatives Considered
|                                         Alternative | Pros                                               | Cons                                                                 | Why Rejected                                                            |
|----------------------------------------------------:|:---------------------------------------------------|:---------------------------------------------------------------------|:------------------------------------------------------------------------|
| Keep manual `Polarity` plus `Debouncer` composition | No new API                                         | Repeated boilerplate; easy to encode active-low logic inconsistently | The composition is common enough to deserve a named primitive           |
|                 Extend `EdgeDetector` with polarity | Reuses existing edge API                           | Emits signal direction instead of semantic presence                  | Presence / absence is clearer for sensors and validation logs           |
|                               Reuse `ButtonDecoder` | Already debounces active-low or active-high inputs | Adds irrelevant gesture semantics                                    | Reed switches and beam breaks are not inherently buttons                |
|                        Create sensor-specific types | Domain-specific names                              | Duplicates the same boolean mapping and debounce behavior            | A generic digital presence primitive covers the shared behavior cleanly |
