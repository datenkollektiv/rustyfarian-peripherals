# Feature: Input Primitives (debounce + rotary) v1

## Decisions
| Decision | Reason | Rejected Alternative |
|----------|--------|----------------------|
| Donate via clean reimplementation, not copy or history graft | Clean fresh-start history; idiomatic to this repo's conventions | git subtree graft (see [ADR-001](../adr/001-input-primitives-origin.md)) |
| `QuadratureDecoder::update -> Option<EncoderDirection>` | Matches the crate's Option-returning, typed-event style | Raw `i32` delta (the knob's original API) |
| `steps_per_detent: u8` with `debug_assert!(> 0)` | Caller can't pass a nonsensical 0/negative; stored as `i8` internally | Signed param exposed directly |
| `hal` adapters generic over `embedded_hal::digital::InputPin`; `B: InputPin<Error = A::Error>` for the rotary | Single error type in scope, no boxing; trivially satisfied on ESP-IDF (`EspError` everywhere) | Phantom error param / boxed error |
| Ship `MockInputPin` under the `hal` feature | "A `Noop*`/mock ships with every trait" — downstream tests reuse ours | Let consumers invent their own mock |

## Constraints
- `no_std`, MSRV 1.88; pure core has zero hardware dependencies.
- `embedded-hal` is pulled in only behind the optional `hal` feature.
- All decode/timing logic stays host-testable in `tamer` (sans-io boundary).

## Open Questions
- [ ] `button` events (click / long-press / double-click) — next slice, on top of `debounce`.
- [ ] `touch` (CST816S) and `display` (GC9A01 / OLED) primitives — later slices.
- [ ] First device example (`idf_s3_crowpanel`) wiring the esp-idf tier — needs the first hardware dependency (`esp-idf-hal`) pinned in `[workspace.dependencies]`.

## State
- [x] Design approved
- [x] Core implementation (`tamer::debounce`, `tamer::rotary`)
- [x] Tests passing (37 default / 53 with `--features hal`)
- [x] Documentation updated (module docs, `prelude`, ADR-001, CHANGELOG)

## Session Log
- 2026-06-22 — Feature doc created; debounce + rotary donated from `rustyfarian-knob` (`zoetrope`) and `rustbox-peripherals` (`rustbox-peripherals-pure`) at source rev `a169dd8`. See [ADR-001](../adr/001-input-primitives-origin.md).
