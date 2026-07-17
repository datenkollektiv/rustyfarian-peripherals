# Roadmap

*Last updated: July 16, 2026*

A re-derived vision broadened this repo from input-only peripherals to a single
home for **all** hardware peripherals — input *and* output (buttons, encoders,
buzzers, displays, LEDs) — so a new device never means a new repo. The workspace
skeleton, tooling, and CI are in place; every crate builds on the host. The pure
`tamer` core plus thin esp-hal / esp-idf tiers is the non-negotiable spine, now
extended to output (tone sequencing, text layout, framebuffer diffing). This
roadmap is **fuzzy by design**: peripherals are added when a real downstream
project needs them, and the order reflects likely demand, not a commitment.
Open questions: whether `rustyfarian-ws2812` folds in, and whether battery /
power devices count as peripherals here (see [VISION.md](../VISION.md)).

Shipped milestones are recorded in [`CHANGELOG.md`](../CHANGELOG.md); this roadmap
tracks only upcoming work.

```mermaid
%%{init: {
  "theme": "base",
  "themeVariables": {
    "cScale0": "#e8f5e9",
    "cScaleLabel0": "#2e7d32",
    "cScale1": "#c8f7c5",
    "cScaleLabel1": "#1b5e20",
    "cScale2": "#fff3cd",
    "cScaleLabel2": "#7a5a00",
    "cScale3": "#e3f2fd",
    "cScaleLabel3": "#0d47a1"
  }
}}%%

timeline
    title rustyfarian-peripherals Roadmap

    Near term : MPU6050 hardware example twin — repo's first I2C example (hal/idf c3, burst read → tilt)
              : Docs-sync — align README / AGENTS framing with VISION input+output scope

    Mid term  : IRAM-safe encoder ISR — run from SRAM for OTA / flash-cache-off safety

    Long term : Display UI logic — touch hit-testing + framebuffer dirty-rect diffing (reuse embedded-graphics; ADR-008)
              : Decide — fold ws2812 in vs. keep sibling (at next real LED use)
              : Ecosystem currency — new chips / HAL waves
```

---

## Architecture Decisions (Frozen)

These drive every peripheral below — input *and* output.

- **Sans-io boundary:** all decode/render/timing logic lives in `tamer` (pure,
  `no_std`, host-testable). The hardware crates are thin wrappers that read pins
  and push bytes. Nothing host-testable goes inside an esp-hal/esp-idf wrapper —
  output peripherals included (tone sequencing, text layout, framebuffer diffing).
- **Trait-first + mocks:** every hardware interaction is behind a trait, and
  every trait ships its `Noop*` mock in the same change.
- **`embedded-hal` is the seam:** adapters read `embedded_hal::digital::InputPin`
  (and the relevant output/bus traits) behind `tamer`'s `hal` feature and feed
  the pure logic. The pure core leans on `embedded-hal` rather than reinventing it.
- **Two hardware tiers, mirrored layout:** `rustyfarian-esp-hal-peripherals`
  (bare-metal) and `rustyfarian-esp-idf-peripherals` (std) keep parallel module
  structure so a peripheral added to one has an obvious home in the other.
- **Demand-driven:** no peripheral lands without a real consumer.

---

## Long term — Display UI logic

**Goal:** Own only the *pure UI-logic gaps* above a display — not a display
abstraction. Rendering is `embedded-graphics` (`DrawTarget`), text and box-layout
are `embedded-text` / `embedded-layout`, and per-board bus / rotation / backlight
glue lives in the chip tiers. `tamer`'s reserved `display` slot holds just the
pure, host-testable logic those don't provide.

**Likely shape (two modules, on demand):**

- **Touch-region hit-testing** (`TouchPoint` → region id) — composes the landed
  `tamer::touch`, needs no `DrawTarget`, and is the first module: it makes touch
  actionable by routing display-space points to tappable regions.
- **Framebuffer dirty-rect diffing** — rect union / merge, the gap
  `embedded-graphics-framebuf` leaves; lands when a driver-tier consumer measures
  redraw cost.

`tamer` takes **no `embedded-graphics` dependency**, even feature-gated.
Sequencing is demand-driven: land the CYD / S3-knob `mipidsi` +
`embedded-graphics` driver in the chip tier first (prerequisite), then donate
each module as a consumer needs it, freezing either API only at a second
consumer. This **refines** the earlier "Character Display" framing (glyph maps +
line layout built in `tamer`) to *reuse* — see
[ADR-008](adr/008-display-scope.md).

---

## Long term — `ws2812` Merge Decision

**Goal:** Decide whether `rustyfarian-ws2812` (WS2812 / NeoPixel effects) folds
into this repo as just-another-output-peripheral, or stays a sibling. It was the
first peripheral and predates this repo's broadened scope. Resolve when the
boundary is next tested by a real LED consumer (see [VISION.md](../VISION.md)).

---

## Mid term — IRAM-Safe Interrupt Handler

**Goal:** Run the encoder ISR in on-chip SRAM so it can service edges even when the
flash cache is disabled (NVS / OTA operations). Today the encoder is **not IRAM-safe**
and can crash if an edge arrives during a flash write.

**Status: Skeleton and scope documented** (see
[Feature: IRAM-Safe ISR v1](features/iram-safe-isr-v1.md)).
Roadmapped as a follow-up when a consumer needs OTA without encoder glitch.
Decisions pending: compile-time opt-in vs. always-on; `QUAD_TABLE` placement (tamer or esp-idf tier).

---

## Open Questions

| Question                                                                                                                | Blocks                   | How to resolve                                                                                                                                                                         |
|:------------------------------------------------------------------------------------------------------------------------|:-------------------------|:---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Fold `ws2812` in vs. keep it a sibling?                                                                                 | ws2812 merge decision    | Decide at the next real LED consumer                                                                                                                                                   |
| ~~Constructor asymmetry: `TouchTracker::new(config)` vs `ButtonDecoder::new(positional args)`~~ **Resolved 2026-07-17** | —                        | Positional `new(long_press, swipe_min_distance, move_epsilon)`, `TouchConfig` deleted — no asymmetry, the family stays positional ([feature Q6](features/touch-event-detection-v1.md)) |
| Do battery / charging devices count as peripherals here?                                                                | Power-device drivers     | Lean `rustyfarian-power`; revisit if a charging IC needs a driver                                                                                                                      |
| Which esp-hal / esp-idf wave to pin to?                                                                                 | First hardware driver    | Match the wave the sibling repos are on (see their `[workspace.dependencies]`)                                                                                                         |
