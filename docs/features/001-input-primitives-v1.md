# Feature: Input Primitives (debounce + presence + rotary + button) v1

## Decisions
|                                                                                                                                                       Decision | Reason                                                                                                             | Rejected Alternative                                                                   |
|---------------------------------------------------------------------------------------------------------------------------------------------------------------:|:-------------------------------------------------------------------------------------------------------------------|:---------------------------------------------------------------------------------------|
|                                                                                                   Donate via clean reimplementation, not copy or history graft | Clean fresh-start history; idiomatic to this repo's conventions                                                    | git subtree graft (see [ADR-001](../adr/001-input-primitives-origin.md))               |
|                                                                                                        `QuadratureDecoder::update -> Option<EncoderDirection>` | Matches the crate's Option-returning, typed-event style                                                            | Raw `i32` delta (the knob's original API)                                              |
|                                                                        `steps_per_detent: u8`, always-on `assert!(> 0)`, accumulator/threshold stored as `i32` | Validation fires in release too; an `i32` accumulator can't overflow for any `u8`, so no upper-bound cap is needed | `debug_assert!` only / `i8` accumulator with a `1..=127` cap                           |
|                                                                               `Debouncer` `0` window = no debouncing (transitions on the first changed sample) | Least-surprising "off" semantic; behavior-neutral for any positive window                                          | Two-call delay even at `0`                                                             |
|  `ButtonDecoder`: raw `Press`/`Release` on every debounced edge; `Click`/`DoubleClick`/`LongPress` layered on top (gesture emitted the tick after the release) | Broadly useful — raw-edge consumers and gesture consumers both served; symmetric `Press`/`Release`                 | Knob's model: `Release` only after a long press (taps emit only `Click`/`DoubleClick`) |
|                                                                                   Button decode built on the existing `EdgeDetector`, not a re-rolled debounce | Reuse; gesture layer never sees contact bounce                                                                     | Duplicate debounce logic inside `button`                                               |
|                                                                                                          `DigitalPresence` composes `Polarity` and `Debouncer` | Raw GPIO in, semantic present / absent transitions out; keeps generic sensors out of button gesture semantics      | Reuse `ButtonDecoder` for reed switches / beam breaks                                  |
| `hal` adapters generic over `embedded_hal::digital::InputPin`; `B: InputPin<Error = A::Error>` for the rotary; `try_from_pin(s)` seeds state from the live pin | Single error type in scope, no boxing; avoids the explicit-`initial` desync footgun                                | Phantom error param / boxed error; `new`-only constructors                             |
|                                                                                                                    Ship `MockInputPin` under the `hal` feature | "A `Noop*`/mock ships with every trait" — downstream tests reuse ours                                              | Let consumers invent their own mock                                                    |

## Constraints
- `no_std`, MSRV 1.88; pure core has zero hardware dependencies.
- `embedded-hal` is pulled in only behind the optional `hal` feature.
- All decode/timing logic stays host-testable in `tamer` (sans-io boundary).

## Open Questions
- [x] `button` events (click / long-press / double-click) — landed on top of `debounce`.
- [ ] `touch` (CST816S) and `display` (GC9A01 / OLED) primitives — later slices.
- [x] First hardware dependency wired — `esp-hal` / `esp-idf-hal` pinned in `[workspace.dependencies]`, both esp tiers carry their HALs, with ESP32-C3 B3F button examples (`hal_c3_b3f`, `idf_c3_b3f`) harvested from `rustbox-peripherals`.
- [ ] Richer device example (`idf_s3_crowpanel`) and the `touch`/`display` primitives it needs — later slice.

## State
- [x] Design approved
- [x] Core implementation (`tamer::debounce`, `tamer::presence`, `tamer::rotary`, `tamer::button`)
- [x] Tests passing (63 default / 86 with `--features hal`)
- [x] Documentation updated (module docs, `prelude`, ADR-001, ADR-002, CHANGELOG)

## Session Log
- 2026-06-22 — debounce + rotary donated from `rustyfarian-knob` (`zoetrope`) and `rustbox-peripherals` (`rustbox-peripherals-pure`) at source rev `a169dd8`. See [ADR-001](../adr/001-input-primitives-origin.md).
- 2026-06-23 — `button` donated from `zoetrope`'s `button_state_machine`; event contract diverges from the knob — raw `Press`/`Release` on every edge plus layered `Click`/`DoubleClick`/`LongPress` gestures (see [ADR-001](../adr/001-input-primitives-origin.md)).
- 2026-06-23 — first hardware dependency wired: both esp tiers gain their HALs and ESP32-C3 B3F button examples (`hal_c3_b3f`, `idf_c3_b3f`) harvested from `rustbox-peripherals`. Host gates narrow to `tamer`; the esp tiers check via `just check-hal` / `check-idf`.
- 2026-06-23 — adopted the esp-hal / esp-idf target split from `rustyfarian-ws2812` and `rustyfarian-network`: separate `target/hal` and `target/idf` dirs (RAM-disk aware) and `scripts/build-example.sh` / `flash.sh` routing by the `{tier}_{chip}_{name}` example-name convention.
- 2026-06-24 — `DigitalPresence` donated from `rustbox-peripherals-pure`'s `input::presence`; adapted to `tamer`'s zero-debounce behavior and `hal` adapter pattern (see [ADR-002](../adr/002-digital-presence.md)).
