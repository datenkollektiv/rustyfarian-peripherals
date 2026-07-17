# ADR-007: Touch event detection (`tamer::touch`)

## Status
Proposed

## Context
`tamer`'s module roadmap reserves a `touch` slot ("capacitive touch event
detection"), and [ADR-001](001-input-primitives-origin.md) explicitly named
`rustyfarian-knob`'s `touch_packet.rs` as a planned future donation, to follow
the recipe established by the debounce, rotary, and button donations.

The knob's `zoetrope::touch_packet` mixes two concerns:

1. **Chip-specific decoding** — the CST816S 6-byte packet layout (register
   offsets, 12-bit coordinate packing across high-nibble/low-byte pairs, and
   the controller's native hardware gesture codes `0x01`–`0x0C`).
2. **Generic event detection** — the `TouchEvent` / `TouchPoint` vocabulary and
   a small state machine that turns a `touched` flag into `Down` / `Up` /
   `Move` edges using the previous frame's state.

Only concern (2) is chip-independent and belongs in `tamer`'s pure core.
Concern (1) is glue specific to one controller.

Two known consumers shape the design:

- **The rotary knob** (`rustyfarian-knob`): a round 240×240 panel driven by a
  CST816S — capacitive, I²C, with a native hardware gesture engine.
- **The "Cheap Yellow Display"** (CYD, ESP32-2432S028R): a rectangular 320×240
  panel driven by an XPT2046 — resistive, SPI, with **no** gesture engine and
  markedly noisier touch state and coordinates.

There is also a structural difference from the existing input primitives.
`debounce`, `rotary`, and `button` each wrap one or two
`embedded_hal::digital::InputPin`s under the `hal` feature.
A touch panel has **no standard `embedded-hal` trait** — it is a bus device
(I²C or SPI) speaking a chip-specific protocol.
So `tamer::touch` cannot offer a uniform `hal` adapter the way the others do;
the chip driver decodes its own packet and feeds coordinates into the pure
tracker.

## Decision
Add `tamer::touch` as a **pure, clock-injected touch-event detector**.
Chip packet decoding stays out of `tamer`.

Proposed surface (architectural sketch — **not the normative API**):

> This ADR fixes the *architecture and constraints*. The concrete v1 API
> contract — payload-carrying events, the collapsed `Swipe(SwipeDirection)`
> variant, and the `update` signature — is owned by
> [Feature: Touch Event Detection v1](../features/touch-event-detection-v1.md).
> Where the sketch below and that feature doc disagree, **the feature doc wins**;
> the divergences are called out inline.

- `TouchEvent { Down, Up, Move, Tap, LongPress, SwipeUp, SwipeDown, SwipeLeft,
  SwipeRight }` — the generic touch vocabulary. *(Superseded: the feature doc
  collapses the four swipes to `Swipe(SwipeDirection)` and gives the edge/gesture
  events a `TouchPoint` payload.)*
- `TouchTracker` — pure, `no_std`, `Copy`, with:
  - `new(config) -> Self` *(Superseded: positional
    `new(long_press, swipe_min_distance, move_epsilon)` — feature Q6, resolved
    2026-07-17)*
  - `update(&mut self, touched: bool, x: i16, y: i16, now: u64) -> Option<TouchEvent>`
    *(Superseded: `update(touch: Option<TouchPoint>, now)` — feature Q1,
    resolved 2026-07-17)*
  - `is_touched(&self) -> bool`
- Detection rules, all from `(touched, x, y, now)`:
  - **Down / Up / Move** from `touched`-edge detection (this frame vs the
    stable previous frame).
  - **LongPress** when held without movement beyond `move_epsilon` for at
    least `long_press` ticks — clock-injected with `saturating_sub`, mirroring
    `tamer::button` and `tamer::debounce`.
  - **Tap** when released quickly with little movement (the short-press
    counterpart to `LongPress`).
  - **Swipe{Up,Down,Left,Right}** derived on release from the net coordinate
    delta over the touch's lifetime: if it exceeds `swipe_min_distance`, the
    dominant axis selects the direction.
- `TouchConfig { long_press, swipe_min_distance, move_epsilon }` (caller ticks
  for time; display units for distances). *(Superseded: `TouchConfig` deleted —
  the parameters go to positional `new`; feature Q2/Q3/Q6, resolved 2026-07-17.)*
- **No `hal` adapter.** The seam is the `(touched, x, y, now)` call; the
  asymmetry with the `InputPin`-based primitives is documented rather than
  papered over with a bespoke trait.
- Fully host-testable by feeding synthetic coordinate streams — no mock pin
  needed.

**Gesture source: derive gestures purely from motion + timing**, *not*
pass-through of a controller's native gesture codes.
This keeps the core chip-independent and host-testable (the whole point of
`tamer`), gives consistent behaviour across controllers, and works on
controllers that report no gestures at all.
The CYD makes this a necessity rather than a principle: the XPT2046 has no
gesture engine, so the pure tracker is the only way that panel gets
swipe / long-press / tap at all.
A consumer that specifically wants a controller's hardware gestures (e.g. the
CST816S gesture engine) can read them from the chip driver directly and bypass
the tracker.

**Coordinate space: the tracker consumes already-calibrated display
coordinates.**
Calibration and rotation mapping stay in the chip/board tier: the CST816S
reports panel pixels directly, while the XPT2046 reports raw 12-bit ADC values
that need per-board calibration before they mean anything in display units.
Swipe directions are defined in the fed coordinate space, so screen rotation
(the CYD is commonly used in landscape) is likewise the caller's job.
This is what makes `swipe_min_distance` / `move_epsilon` "display units"
well-defined across both consumers.

**Noisy touch state: compose, don't absorb.**
Resistive panels flicker their touched/pressure state and jitter coordinates
far more than capacitive ones.
The tracker derives Down/Up from `touched` edges and does **not** grow its own
debounce configuration; a caller with a noisy controller runs the raw
`touched` flag through the existing `tamer::debounce::Debouncer` first.
Coordinate jitter within `move_epsilon` is already absorbed by the epsilon.

**Chip seam.** The CST816S packet decoding (byte layout, coordinate packing,
register map, native gesture codes) is **not** donated.
It stays in the consuming firmware (the knob) for now, and is a candidate for
a future chip-tier home — a CST816S driver in
`rustyfarian-esp-idf-peripherals`, or a dedicated `cst816s` crate.
The CYD's XPT2046 decoding likewise lives in its consuming project (or an
ecosystem driver crate), never in `tamer`.
`tamer::touch` consumes only decoded, calibrated `(touched, x, y)`.

**Provenance.** Clean reimplementation per
[ADR-001](001-input-primitives-origin.md): the generic event/edge logic from
`touch_packet.rs` is relicensed MIT → MIT OR Apache-2.0 and cited; the
CST816S-specific parsing is deliberately not carried over.

## Consequences
**Positive:**

- Completes `tamer`'s input set (debounce, rotary, button, **touch**) with a
  chip-independent, host-testable primitive.
- Richer than the knob's current logic: pure swipe / long-press / tap
  derivation works on any touch controller, not only ones with a hardware
  gesture engine — the CYD's XPT2046 being the concrete case.
- Clean layering — pure detection in `tamer`, chip decoding and calibration in
  the chip tier — matching the rest of the stack.
- One tracker serves both known consumers (round capacitive knob, rectangular
  resistive CYD); round vs rectangular is irrelevant to Cartesian swipe deltas.

**Negative / trade-offs:**

- If the knob adopts it, behaviour changes: it stops trusting the CST816S's
  hardware gesture codes and derives gestures from motion instead.
  This needs hardware re-validation, and `swipe_min_distance` /
  `move_epsilon` need tuning per panel (round 240×240 capacitive vs
  rectangular 320×240 resistive).
- No uniform `hal` adapter (a touch panel is not an `InputPin`), so the seam
  is a plain coordinate-feeding call — asymmetric with the other primitives.
- Noisy controllers need an explicit `Debouncer` composition step in the
  caller; the tracker will not hide that for them.
- The CST816S parsing remains un-homed (stays in the knob) until a chip-tier
  crate exists. This is a partial donation, not a full lift like rotary/button.
- Single-touch only in the first cut (both known consumers are single-finger);
  multitouch is left for a later revision.
- ~~`TouchTracker::new(config)` takes a config struct where `ButtonDecoder::new`
  takes positional arguments — a deliberate asymmetry (3+ related parameters)
  flagged on the roadmap for review before the API freezes.~~ *(Resolved
  2026-07-17: positional `new`, `TouchConfig` deleted — no asymmetry; feature Q6.)*

## Alternatives Considered
|                                                Alternative |                                             Pros | Cons                                                                                                            | Why Rejected                                                                              |
|-----------------------------------------------------------:|-------------------------------------------------:|:----------------------------------------------------------------------------------------------------------------|:------------------------------------------------------------------------------------------|
|        Pass-through chip gesture codes into `tamer::touch` | Matches CST816S hardware exactly; less new logic | Leaks a chip assumption into the pure core; controllers like the XPT2046 report no gestures; not host-derivable | Violates `tamer`'s chip-independent, host-testable principle                              |
| Donate `touch_packet` verbatim (including CST816S parsing) |                           Fastest; a single move | Puts chip-specific byte/register layout in the pure core; not reusable across controllers                       | Wrong layer — `tamer` is chip-agnostic                                                    |
|                            Keep touch entirely in the knob |                                          No work | Misses the planned `tamer::touch`; touch logic gets duplicated in the CYD and future rustbox projects           | Touch is explicitly on `tamer`'s roadmap with two known consumers                         |
|          Add a bespoke `tamer` `hal` touch trait + adapter |           API symmetry with the other primitives | No standard `embedded-hal` touch trait exists; inventing one with no ecosystem behind it is premature           | Revisit only if an ecosystem touch trait emerges                                          |
|              Build calibration / rotation into the tracker |                        One-stop shop for the CYD | Calibration is per-board analog glue, not event logic; bloats the pure core with chip-tier concerns             | Tracker consumes calibrated display coordinates; calibration stays in the chip/board tier |
|                         Built-in `touched` debounce config |         Handles resistive flicker out of the box | Duplicates `tamer::debounce`; grows config for a problem composition already solves                             | Callers compose `Debouncer` → `TouchTracker`                                              |
