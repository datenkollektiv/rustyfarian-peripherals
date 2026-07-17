# ADR-008: Display scope — reuse `embedded-graphics`, keep only pure UI logic in `tamer`

## Status
Accepted

## Context
`tamer`'s module roadmap reserves a `display` slot, framed by the long-term
[Character Display](../ROADMAP.md) item as a "`tamer` text/framebuffer layer —
glyph maps, line layout, and framebuffer diffing" plus a `hal`-feature bus
adapter. With [`tamer::touch`](007-touch-event-detection.md) landed — a pure
tracker that already emits points in **display coordinates** — the natural next
question surfaced from a downstream consumer (the CYD / rustbox-midway journey):
*before adopting touch, do we benefit from a "display abstraction"?*

Four panels are in view as consumers:

- **S3 knob** — round 240×240, GC9A01 + CST816S (capacitive).
- **CYD** (ESP32-2432S028R) — rectangular 320×240, ILI9341 + XPT2046 (resistive).
- **0.96" OLED** — SSD1306 128×64.
- **1.3" OLED** — SH1106 128×64.

Two facts shape the decision. First, "display abstraction" is ambiguous and
splits into three layers with different owners:

1. **Driver / "render to a panel"** — `embedded-graphics`' `DrawTarget` /
   `Drawable`. Every panel above already has a mature `DrawTarget` driver
   (`mipidsi` for ILI9341/ST7789, `gc9a01`, `ssd1306`, `sh1106`).
2. **Chip/board glue** — SPI/I²C init, rotation, backlight, touch calibration.
   Per-board, lives in the chip tiers, exactly like the XPT2046/CST816S touch
   seam from [ADR-007](007-touch-event-detection.md).
3. **Pure UI/render *logic* above `DrawTarget`** — hit-testing, framebuffer
   diffing, text/layout.

Second, [ADR-007](007-touch-event-detection.md) already set the governing
precedent: **no bespoke `tamer` trait where an ecosystem-standard trait exists**
(there, no `embedded-hal` touch trait existed; here, `embedded-graphics` *is* the
de-facto standard). The CYD feature doc
([cyd-support-v1](../features/cyd-support-v1.md)) independently reached the same
conclusion for layer 1: "wrap `mipidsi` + `embedded-graphics` in the hardware
tier; keep the pure crate's value at the layout / dirty-rect-diffing layer" and
rejected hand-written drivers. `embedded-graphics` is already a workspace dep in
the consuming demos.

## Decision
**In one sentence:** `tamer` will not define a display abstraction; it will only
host pure, host-testable UI logic that composes with `embedded-graphics`-based
consumers.

**Layer 1 — reuse, no Rustyfarian display trait.** `embedded-graphics`'
`DrawTarget` / `Drawable` is the display HAL. A `tamer` display trait over it
would be pure duplication of a solved, widely-adopted contract — the exact
anti-pattern [ADR-007](007-touch-event-detection.md) rejects.

**Layer 2 — stays in the chip tiers** (`rustyfarian-esp-idf-peripherals` /
`rustyfarian-esp-hal-peripherals`), as thin per-board glue. Not a shared
abstraction.

**Layer 3 — the only thing `tamer`'s `display` slot holds.** It is pure,
`no_std`, host-testable UI logic that renders *to* a `DrawTarget` and never
replaces it, following the `touch`/`tone` seam (consume plain data, emit
plain data — region ids, dirty rects — the caller applies via *their own*
`embedded-graphics` integration; `tamer` never calls `.draw()`). Scope is
narrowed to two modules, ranked by value; text and box-layout are explicitly
**out of scope**, owned upstream:

| Rank | Module                                                  | Fit                                                                                                                                                                                                                                                                                                                         | When                                                                                      |
|-----:|:--------------------------------------------------------|:----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|:------------------------------------------------------------------------------------------|
|    1 | **Touch-region hit-testing** (`TouchPoint` → region id) | Pure geometry over the `TouchPoint` `tamer` already owns; **no `DrawTarget` / `embedded-graphics` dependency at all**; inherits `touch`'s raw-feed test seam. It is really a *touch consumer*, and it's what makes the just-landed touch actionable — today touch emits display-space points with nothing to route them to. | First `display`-adjacent module. Land when a consumer builds tappable UI (S3 knob / CYD). |
|    2 | **Framebuffer dirty-rect diffing**                      | Real value on the SPI panels (full-frame pushes to 320×240 / round 240×240 are the bottleneck). `embedded-graphics-framebuf` supplies a buffer but **not** the diff/merge algorithm — rect union/merge is the pure gap.                                                                                                     | Land when a driver-tier consumer measures redraw cost.                                    |
|    — | Character/text-console line buffer                      | Consumer-gated. A *scrolling console/log* is genuinely distinct from `embedded-text`'s static `TextBox` and is the natural OLED-as-status-screen use — but only build it if a real OLED consumer needs it.                                                                                                                  | Deferred until an OLED consumer materialises.                                             |
|    — | Text layout / glyph maps / box-model                    | **Reuse.** `embedded-text` (`TextBox`, wrapping, alignment) + `embedded-layout` (linear layout, alignment) own this. Reimplementing in `tamer` is the duplication ADR-007 warns against. This **refines** the ROADMAP "Character Display" item's "glyph maps, line layout" framing to *reuse*.                              |

The `display` label is **historical** — it is the name of the reserved ROADMAP
slot, and the scope above is narrower than the word implies: no drivers,
surfaces, fonts, or box-layout live here. In particular, although grouped under
the slot for roadmap continuity, touch-region hit-testing is conceptually an
*interaction-layer* utility built on display-space coordinates, not a display
abstraction.

**No `embedded-graphics` dependency in `tamer` — not even feature-gated.**
It has breaking colour/trait migrations (0.7 → 0.8) `tamer` would track in
lockstep for zero logic payoff; `DrawTarget` is a trait *drivers* implement and
*apps* consume, not something the pure render-logic needs internally; and every
consuming demo already depends on `embedded-graphics` directly, so a `tamer`-side
dep saves them nothing and only adds a CI feature-matrix cell and a `Cargo.lock`
version to reconcile. The `hal` feature (gating a single `embedded-hal`
`InputPin`) remains the *only* hardware-adjacent seam. This is current policy,
not permanent doctrine: revisit only if a future cross-consumer need for a
`tamer`-side `DrawTarget` integration emerges, held to the same ≥2-consumer bar
applied to every other primitive.

**Demand-driven sequencing.** Consistent with the crate charter (no primitive
without a real consumer; generalise only at ≥2 consumers; donate, don't
speculate): the slot stays *reserved, not implemented*. The CYD display is
unstarted (❌) and the OLEDs are hypothetical. Order: (1) land the CYD / S3-knob
`mipidsi` + `embedded-graphics` driver in the chip tier per the CYD feature doc
(a prerequisite, not this ADR's scope); (2) donate hit-testing the moment a
consumer builds tappable UI; (3) donate dirty-rect diffing when a consumer
measures redraw cost; (4) require a second board hitting the same need before
freezing either API — the same bar applied to `touch`.

The `tamer::lib.rs` pending-slot comment is narrowed to this scope, and the
ROADMAP "Character Display" item is reconciled to match (both done with this
ADR), so a future contributor does not read either as "build a display trait"
and re-litigate the reuse decision.

## Consequences
**Positive:**

- No duplication of `embedded-graphics` / `embedded-text` / `embedded-layout`;
  `tamer` owns only the ecosystem *gaps* (tap→region routing, framebuffer
  diffing) that are genuinely pure logic.
- Hit-testing closes the loop on the just-landed `touch`: it composes directly,
  needs no `DrawTarget`, and is fully host-testable with synthetic `TouchPoint`s
  — the lowest-risk possible first `display`-adjacent module.
- `tamer` stays `embedded-graphics`-free, so no semver coupling to that crate's
  breaking colour/trait migrations; the pure core keeps its single `hal` seam.
- Clean layering matches the rest of the stack (pure logic in `tamer`, drivers
  reused, chip glue in the tiers) and the CYD feature doc's existing decision.

**Negative / trade-offs:**

- The `display` slot delivers less than its ROADMAP "Character Display" framing
  implied: text layout and glyph maps are reused, not built here. The ROADMAP
  item is reconciled in this same change (renamed to "Display UI logic";
  framebuffer diffing kept; text/layout → reuse; hit-testing added as the first
  module).
- Hit-testing and dirty-rect are *two* modules on *demand*, not one display
  layer up front — more sequencing discipline, less to point at today.
- The scrolling-console question is left open and consumer-gated; an OLED
  consumer will have to argue it past `embedded-text` before it lands.

## Alternatives Considered
|                                                                                           Alternative | Pros                                                                   | Cons                                                                                                                    | Why Rejected                                                             |
|------------------------------------------------------------------------------------------------------:|:-----------------------------------------------------------------------|:------------------------------------------------------------------------------------------------------------------------|:-------------------------------------------------------------------------|
|                                                         A Rustyfarian display *trait* over the panels | API symmetry with the family's `hal` adapters                          | Duplicates `embedded-graphics`' `DrawTarget`, an adopted ecosystem standard                                             | Same layer-1 duplication ADR-007 rejects; `embedded-graphics` is the HAL |
| Build the ROADMAP's full "text/framebuffer layer" (glyph maps + line layout + diffing) in `tamer` now | One-stop character-display story; matches the original roadmap wording | `embedded-text`/`embedded-layout` already own layout; speculative ahead of any consumer; violates demand-driven charter | Reuse layout, keep only the diffing gap; land on demand                  |
|                                     Take an optional/feature-gated `embedded-graphics` dep in `tamer` | Could offer ready-made `DrawTarget` glue                               | Semver lockstep to eg's breaking migrations; no logic payoff; consumers already depend on eg directly                   | Zero benefit, real coupling; keep `tamer` eg-free                        |
|                                           Character/text-console buffer as the first `display` module | Serves the two OLEDs directly; a clean zero-alloc grid primitive       | OLEDs are hypothetical consumers; overlaps `embedded-text`; not composed with the just-landed `touch`                   | Deferred and consumer-gated; hit-testing leads instead                   |
|                                                         Implement nothing and drop the `display` slot | No speculative surface                                                 | Loses the genuine gaps (tap→region routing, framebuffer diffing) and the `touch` follow-through                         | Keep the slot reserved with narrowed scope; implement on demand          |
